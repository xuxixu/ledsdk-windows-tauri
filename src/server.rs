//! 后台 HTTP 接口（axum）：接收 JSON + 素材文件，查询作业与设备。

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use axum::extract::{DefaultBodyLimit, Multipart, Path, Query, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use log::{info, warn};
use serde::Deserialize;
use serde_json::json;

use crate::config::ScreenRegistry;
use crate::host::SdkHost;
use crate::jobs::JobQueue;
use crate::models::{JobState, JobStatus, UploadJobRequest};
use crate::runtime::UploadApiStatus;
use utoipa::openapi::security::{HttpAuthScheme, HttpBuilder, SecurityScheme};
use utoipa::{Modify, OpenApi, ToSchema};
use utoipa_swagger_ui::SwaggerUi;

/// 给 OpenAPI 加上 Bearer token 方案（utoipa 5 只能通过 Modify addon 注入，不能写在 components 里）。
struct SecurityAddon;
impl Modify for SecurityAddon {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        let components = openapi.components.get_or_insert_with(Default::default);
        components.security_schemes.insert(
            "bearerAuth".to_string(),
            SecurityScheme::Http(
                HttpBuilder::new()
                    .scheme(HttpAuthScheme::Bearer)
                    .bearer_format("token")
                    .build(),
            ),
        );
    }
}

#[derive(Clone)]
pub struct ApiState {
    pub host: Arc<SdkHost>,
    pub registry: Arc<ScreenRegistry>,
    pub jobs: JobQueue,
    pub listen_address: String,
    pub port: u16,
    /// 可选的共享 token：为空表示不校验（内网受控部署）。
    pub token: Option<String>,
    pub upload_api_status: Arc<UploadApiStatus>,
}

/// 单个上传请求的请求体上限（素材文件本身走磁盘流式写入，不占同等内存）。
const MAX_UPLOAD_BYTES: usize = 1024 * 1024 * 1024;
/// `request` 字段（JSON）的大小上限，正常只有几百字节。
const MAX_REQUEST_FIELD_BYTES: usize = 64 * 1024;

/// OpenAPI 文档（Swagger UI 用）：接口调试页在 /swagger-ui，规范在 /api-docs/openapi.json。
#[derive(OpenApi)]
#[openapi(
    info(
        title = "LedSdk 屏幕节目上传接口",
        version = "1.0.0",
        description = "上传 JSON + 素材文件 → 建节目树 → 校验 → 导出 → 发送到屏幕。所有响应为 UTF-8 JSON。"
    ),
    paths(health, list_screens, list_jobs, get_job, set_geometry, upload_program),
    components(schemas(
        HealthResponse,
        JobStatus,
        JobState,
        ScreenView,
        GeometryRequest,
        GeometryResponse,
        UploadProgramForm,
        ApiError
    )),
    modifiers(&SecurityAddon)
)]
struct ApiDoc;

pub fn router(state: ApiState) -> Router {
    // 写接口单独一组，先过 token 中间件：multipart 解析失败时 axum 会直接在提取器阶段返回 400，
    // 放在 handler 里的校验根本轮不到执行，所以鉴权必须在进入提取器之前。
    let protected = Router::new()
        .route("/api/program", post(upload_program))
        .route("/api/screens/{device_id}/geometry", post(set_geometry))
        .route_layer(axum::middleware::from_fn_with_state(
            state.clone(),
            require_token,
        ));

    Router::new()
        .route("/api/health", get(health))
        .route("/api/jobs", get(list_jobs))
        .route("/api/jobs/{job_id}", get(get_job))
        .route("/api/screens", get(list_screens))
        .merge(protected)
        .merge(SwaggerUi::new("/swagger-ui").url("/api-docs/openapi.json", ApiDoc::openapi()))
        .with_state(state)
        // axum 默认请求体上限只有 2 MB，会让稍大的图片/视频全部解析失败。
        .layer(DefaultBodyLimit::max(MAX_UPLOAD_BYTES))
}

/// 写接口的 Bearer token 校验（`UploadApi.Token` 为空时不校验）。
async fn require_token(
    State(state): State<ApiState>,
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> Response {
    if let Err(message) = check_token(&state, request.headers()) {
        return reject(StatusCode::UNAUTHORIZED, message);
    }
    next.run(request).await
}

pub async fn serve(state: ApiState, shutdown: Arc<AtomicBool>) -> std::io::Result<()> {
    let address = state.listen_address.clone();
    let port = state.port;
    let display = crate::net::browsable_address(&address);
    // 直接拼 "{ip}:{port}" 在 IPv6 下会得到 ":::5180"：先解析成 SocketAddr 再绑定。
    let listener = match address.parse::<std::net::IpAddr>() {
        Ok(ip) => tokio::net::TcpListener::bind(std::net::SocketAddr::new(ip, port)).await?,
        Err(_) => tokio::net::TcpListener::bind(format!("{address}:{port}")).await?,
    };
    state.upload_api_status.mark_running();
    let api_status = state.upload_api_status.clone();
    info!(
        "上传接口 http://{display}:{port}（示例：POST /api/program；接口调试：http://{display}:{port}/swagger-ui/#/）"
    );
    // 优雅关闭：退出时置位 signal，停止接收新连接并排空在途请求。
    let result = axum::serve(listener, router(state))
        .with_graceful_shutdown(async move {
            while !shutdown.load(Ordering::SeqCst) {
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            info!("上传接口收到关闭信号，停止接收新请求。");
        })
        .await;
    api_status.mark_stopped();
    result
}

#[derive(serde::Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct HealthResponse {
    /// 当前平台是否有可用的 LedSDK（Windows x64 / Linux x64）。
    pub sdk_available: bool,
    /// SDK 版本与 ABI，例如 "1.0.0 (ABI 2)"
    pub sdk: String,
    /// SDK HTTP 文件服务是否在运行（设备要靠它下载节目包）
    pub http_running: bool,
    pub http_port: u16,
    /// 进行中任务数（含常驻搜索）
    pub active_task_count: u32,
    /// 常驻搜索任务数
    pub search_task_count: u32,
    /// 配置清单里的屏幕数量
    pub screens: usize,
}

#[derive(serde::Serialize, ToSchema)]
pub struct ApiError {
    /// 中文错误说明
    pub error: String,
}

#[utoipa::path(
    get,
    path = "/api/health",
    tag = "系统",
    responses((status = 200, body = HealthResponse, description = "SDK / HTTP 服务 / 作业计数 / 屏幕数量"))
)]
async fn health(State(state): State<ApiState>) -> Response {
    let status = state.host.runtime_status();
    let body = HealthResponse {
        sdk_available: state.host.available(),
        sdk: state.host.version(),
        http_running: state.host.http_running(),
        http_port: state.host.http_port(),
        active_task_count: status.as_ref().map(|s| s.active_task_count).unwrap_or(0),
        search_task_count: status.as_ref().map(|s| s.search_task_count).unwrap_or(0),
        screens: state.registry.screens().len(),
    };
    (StatusCode::OK, Json(body)).into_response()
}

#[derive(serde::Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ScreenView {
    pub name: String,
    pub device_id: String,
    pub ip: String,
    pub port: u16,
    pub device_type: u32,
    pub width: i32,
    pub height: i32,
    pub rotate: i32,
    /// 收到该设备回包时命中的本机网卡 IPv4（下载地址就是它）
    pub local_address: String,
    pub online: bool,
    pub last_seen: Option<String>,
    pub source: crate::config::ScreenSource,
    /// 宽高是否已补录（0x0 表示还没量过屏幕）
    pub has_geometry: bool,
}

#[utoipa::path(
    get,
    path = "/api/screens",
    tag = "屏幕",
    responses((status = 200, body = Vec<ScreenView>, description = "配置清单里的屏幕"))
)]
async fn list_screens(State(state): State<ApiState>) -> Response {
    (StatusCode::OK, Json(screen_views(&state.registry))).into_response()
}

/// 屏幕清单的唯一视图来源：HTTP `/api/screens` 与桌面窗口的 `list_screens` 都用它。
pub fn screen_views(registry: &crate::config::ScreenRegistry) -> Vec<ScreenView> {
    registry
        .screens()
        .into_iter()
        .map(|s| ScreenView {
            name: s.name.clone(),
            device_id: s.device_id.clone(),
            ip: s.ip.clone(),
            port: s.port,
            device_type: s.device_type,
            width: s.width,
            height: s.height,
            rotate: s.rotate,
            local_address: s.local_address.clone(),
            online: s.online,
            last_seen: s.last_seen.clone(),
            source: s.source,
            has_geometry: s.has_geometry(),
        })
        .collect()
}

#[derive(Deserialize, ToSchema)]
pub struct JobsQuery {
    /// 返回最近多少条，默认 20（1-200）
    limit: Option<usize>,
}

#[utoipa::path(
    get,
    path = "/api/jobs",
    tag = "作业",
    params(("limit" = Option<usize>, Query, description = "返回最近多少条，默认 20（1-200）")),
    responses((status = 200, body = Vec<JobStatus>, description = "最近的作业，按提交时间倒序"))
)]
async fn list_jobs(State(state): State<ApiState>, Query(query): Query<JobsQuery>) -> Response {
    let limit = query.limit.unwrap_or(20).clamp(1, 200);
    (StatusCode::OK, Json(state.jobs.recent(limit))).into_response()
}

#[utoipa::path(
    get,
    path = "/api/jobs/{job_id}",
    tag = "作业",
    params(("job_id" = String, Path, description = "提交接口返回的 jobId")),
    responses(
        (status = 200, body = JobStatus, description = "作业状态"),
        (status = 404, body = ApiError, description = "作业不存在")
    )
)]
async fn get_job(State(state): State<ApiState>, Path(job_id): Path<String>) -> Response {
    match state.jobs.status(&job_id) {
        Some(status) => (StatusCode::OK, Json(status)).into_response(),
        None => reject(StatusCode::NOT_FOUND, format!("作业 {job_id} 不存在。")),
    }
}

#[derive(Deserialize, ToSchema)]
pub struct GeometryRequest {
    /// 屏幕宽（像素）
    #[serde(default)]
    width: i32,
    /// 屏幕高（像素）
    #[serde(default)]
    height: i32,
    /// 旋转角度（0/90/180/270）
    #[serde(default)]
    rotate: i32,
}

#[derive(serde::Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct GeometryResponse {
    pub device_id: String,
    pub width: i32,
    pub height: i32,
    pub rotate: i32,
}

#[utoipa::path(
    post,
    path = "/api/screens/{device_id}/geometry",
    tag = "屏幕",
    security(("bearerAuth" = [])),
    params(("device_id" = String, Path, description = "设备的 deviceId（可用 GET /api/screens 查）")),
    request_body = GeometryRequest,
    responses(
        (status = 200, body = GeometryResponse, description = "已补录并写回配置"),
        (status = 400, body = ApiError, description = "宽/高不合法"),
        (status = 401, body = ApiError, description = "Bearer token 缺失或错误"),
        (status = 404, body = ApiError, description = "设备不在清单中")
    )
)]
async fn set_geometry(
    State(state): State<ApiState>,
    Path(device_id): Path<String>,
    Json(body): Json<GeometryRequest>,
) -> Response {
    if body.width <= 0 || body.height <= 0 {
        return reject(
            StatusCode::BAD_REQUEST,
            "width/height 必须大于 0。".to_string(),
        );
    }
    if !matches!(body.rotate, 0 | 90 | 180 | 270) {
        return reject(
            StatusCode::BAD_REQUEST,
            "rotate 只能是 0/90/180/270。".to_string(),
        );
    }
    if state.registry.resolve_by_device_id(&device_id).is_none() {
        return reject(
            StatusCode::NOT_FOUND,
            format!("设备 '{device_id}' 不在配置清单中（可用 GET /api/screens 查看 deviceId）。"),
        );
    }
    if !state
        .registry
        .set_geometry(&device_id, body.width, body.height, body.rotate)
    {
        return reject(
            StatusCode::NOT_FOUND,
            format!("设备 '{device_id}' 不存在或已被移除。"),
        );
    }
    info!(
        "已补录几何：设备={device_id} {}x{} 旋转={}°",
        body.width, body.height, body.rotate
    );
    (
        StatusCode::OK,
        Json(GeometryResponse {
            device_id,
            width: body.width,
            height: body.height,
            rotate: body.rotate,
        }),
    )
        .into_response()
}

/// POST /api/program 的 multipart 表单：`request` 是业务 JSON 字符串，`file` 是素材文件。
#[derive(ToSchema)]
#[allow(dead_code)]
pub struct UploadProgramForm {
    /// 业务 JSON 字符串，例如 {"fileName":"a.png","fileType":"picture","deviceId":"ZH-M1-4EBC5A23","x":0,"y":0,"width":256,"height":256}
    pub request: String,
    /// 素材文件（二进制）
    #[schema(value_type = String, format = Binary)]
    pub file: Vec<u8>,
}

#[utoipa::path(
    post,
    path = "/api/program",
    tag = "上传",
    security(("bearerAuth" = [])),
    request_body(content = UploadProgramForm, content_type = "multipart/form-data"),
    responses(
        (status = 202, body = JobStatus, description = "已接收，异步建树/校验/导出/发送；Location 头指向作业"),
        (status = 400, body = ApiError, description = "表单或 request 不合法"),
        (status = 401, body = ApiError, description = "Bearer token 缺失或错误")
    )
)]
async fn upload_program(
    State(state): State<ApiState>,
    headers: HeaderMap,
    mut multipart: Multipart,
) -> Response {
    // 非 Windows 平台没有 LedSDK：早点返回 503，避免把文件收完才失败。
    if !state.host.available() {
        return reject(
            StatusCode::SERVICE_UNAVAILABLE,
            "本平台没有可用的 LedSDK（仅 Windows x64 / Linux x64），无法建树/导出/发送。"
                .to_string(),
        );
    }
    if headers
        .get(header::CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
        .is_some_and(|length| length > MAX_UPLOAD_BYTES as u64)
    {
        return reject(
            StatusCode::PAYLOAD_TOO_LARGE,
            format!("请求体超过上限 {} MB。", MAX_UPLOAD_BYTES / 1024 / 1024),
        );
    }
    let upload_root = PathBuf::from(&state.registry.sdk().upload_root);
    if let Err(err) = std::fs::create_dir_all(&upload_root) {
        return reject(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("创建上传目录失败：{}：{err}", upload_root.display()),
        );
    }

    let mut request_text: Option<String> = None;
    // 一个请求只接受一个文件：request JSON 也只描述一个素材，多传等于静默丢文件。
    let mut file_received = false;
    // 出错时由 Drop 删除半截文件；成功交给队列后 disarm。
    let mut staged = StagedFile::default();

    loop {
        let mut field = match multipart.next_field().await {
            Ok(Some(field)) => field,
            Ok(None) => break,
            Err(err) => {
                return reject(StatusCode::BAD_REQUEST, format!("表单解析失败：{err}"));
            }
        };
        let name = field.name().unwrap_or_default().to_string();

        if name == "request" {
            // request 只是几百字节的 JSON；不设上限时一个超大字段就能吃掉整个请求体上限的内存。
            let mut buffer: Vec<u8> = Vec::new();
            loop {
                match field.chunk().await {
                    Ok(Some(chunk)) => {
                        if buffer.len() + chunk.len() > MAX_REQUEST_FIELD_BYTES {
                            return reject(
                                StatusCode::BAD_REQUEST,
                                "request 字段过大（上限 64 KB）。".to_string(),
                            );
                        }
                        buffer.extend_from_slice(&chunk);
                    }
                    Ok(None) => break,
                    Err(err) => {
                        return reject(StatusCode::BAD_REQUEST, format!("读取表单字段失败：{err}"));
                    }
                }
            }
            match String::from_utf8(buffer) {
                Ok(text) => request_text = Some(text),
                Err(_) => {
                    return reject(
                        StatusCode::BAD_REQUEST,
                        "request 字段必须是 UTF-8 文本。".to_string(),
                    );
                }
            }
            continue;
        }

        let Some(file_name) = field.file_name().map(|n| n.to_string()) else {
            continue;
        };
        if file_name.is_empty() {
            continue;
        }

        if file_received {
            return reject(
                StatusCode::BAD_REQUEST,
                "一次只能上传一个文件字段（request 只描述一个素材）。".to_string(),
            );
        }
        file_received = true;

        // 素材边收边写磁盘：大文件不整块进内存。
        let extension = std::path::Path::new(&file_name)
            .extension()
            .map(|e| format!(".{}", e.to_string_lossy()))
            .unwrap_or_default();
        let path = upload_root.join(format!("{}{extension}", crate::jobs::new_id()));
        if let Err(err) = staged.begin(&path) {
            return reject(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("创建上传文件失败：{}：{err}", path.display()),
            );
        }
        let mut sink = match tokio::fs::File::create(&path).await {
            Ok(file) => file,
            Err(err) => {
                return reject(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("创建上传文件失败：{}：{err}", path.display()),
                );
            }
        };
        let mut size = 0u64;
        let mut field = field;
        loop {
            match field.chunk().await {
                Ok(Some(chunk)) => {
                    if let Err(err) = tokio::io::AsyncWriteExt::write_all(&mut sink, &chunk).await {
                        return reject(
                            StatusCode::INTERNAL_SERVER_ERROR,
                            format!("写入上传文件失败：{}：{err}", path.display()),
                        );
                    }
                    size += chunk.len() as u64;
                }
                Ok(None) => break,
                Err(err) => {
                    return reject(StatusCode::BAD_REQUEST, format!("读取上传文件失败：{err}"));
                }
            }
        }
        if let Err(err) = tokio::io::AsyncWriteExt::flush(&mut sink).await {
            return reject(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("写入上传文件失败：{}：{err}", path.display()),
            );
        }
        drop(sink);
        staged.replace(file_name, path, size);
    }

    let Some(request_text) = request_text else {
        return reject(
            StatusCode::BAD_REQUEST,
            "缺少表单字段 request（JSON 字符串）。".to_string(),
        );
    };

    let request: UploadJobRequest = match serde_json::from_str(&request_text) {
        Ok(request) => request,
        Err(err) => {
            return reject(
                StatusCode::BAD_REQUEST,
                format!("request 不是合法 JSON：{err}"),
            )
        }
    };

    info!(
        "收到上传：文件名={} 类型={} 设备={} 窗口=[{},{},{}x{}]",
        request.file_name,
        request.file_type,
        request.device_id,
        request.x,
        request.y,
        request.width,
        request.height
    );

    if request.file_name.trim().is_empty()
        || request.file_type.trim().is_empty()
        || request.device_id.trim().is_empty()
    {
        return reject(
            StatusCode::BAD_REQUEST,
            "fileName / fileType / deviceId 都不能为空。".to_string(),
        );
    }
    if !matches!(
        request.file_type.trim().to_ascii_lowercase().as_str(),
        "picture" | "image" | "gif" | "video" | "text"
    ) {
        return reject(
            StatusCode::BAD_REQUEST,
            format!(
                "不支持的 fileType '{}'（可用 picture / image / gif / video / text）。",
                request.file_type
            ),
        );
    }
    if request.x < 0 || request.y < 0 || request.width <= 0 || request.height <= 0 {
        return reject(
            StatusCode::BAD_REQUEST,
            "x/y 不能为负，width/height 必须大于 0。".to_string(),
        );
    }

    let screen = match state.registry.resolve_by_device_id(&request.device_id) {
        Some(screen) => screen,
        None => {
            return reject(
                StatusCode::BAD_REQUEST,
                format!(
                    "设备 '{}' 不在配置清单中（可用 GET /api/screens 查看 deviceId）。",
                    request.device_id
                ),
            );
        }
    };
    if !screen.has_geometry() {
        return reject(
            StatusCode::BAD_REQUEST,
            format!(
                "设备 '{}' 还没有屏幕尺寸（宽/高为 0），请先 POST /api/screens/{}/geometry 补录。",
                screen.name, request.device_id
            ),
        );
    }

    let Some((file_name, path, size)) = staged.take() else {
        return reject(
            StatusCode::BAD_REQUEST,
            format!(
                "没有找到与 fileName '{}' 匹配的上传文件。",
                request.file_name
            ),
        );
    };
    if file_name != request.file_name {
        warn!(
            "上传文件名 {file_name} 与 request.fileName {} 不一致，以实际上传的文件为准。",
            request.file_name
        );
    }
    if size == 0 {
        let _ = std::fs::remove_file(&path);
        return reject(StatusCode::BAD_REQUEST, "上传文件为空。".to_string());
    }

    let status = state.jobs.submit(request, path);
    let location = format!("/api/jobs/{}", status.job_id);
    (
        StatusCode::ACCEPTED,
        [(header::LOCATION, location)],
        Json(status),
    )
        .into_response()
}

/// 已落盘的上传文件：任何提前返回都会删掉它，成功入队前调用 `take()` 交出所有权。
#[derive(Default)]
struct StagedFile {
    inner: Option<(String, PathBuf, u64)>,
}

impl StagedFile {
    fn begin(&mut self, path: &PathBuf) -> std::io::Result<()> {
        // 先登记路径，创建失败时也能被 Drop 清掉。
        // 一次请求带多个 file 字段时，上一个已落盘的文件要删掉，否则永久留在 upload/。
        if let Some((_, previous, _)) = self.inner.replace((String::new(), path.clone(), 0)) {
            if previous != *path {
                let _ = std::fs::remove_file(previous);
            }
        }
        Ok(())
    }

    fn replace(&mut self, file_name: String, path: PathBuf, size: u64) {
        if let Some((_, previous, _)) = self.inner.replace((file_name, path.clone(), size)) {
            // begin() 已经登记了同一个路径，不能把它当成「上一个待删文件」删掉。
            if previous != path {
                let _ = std::fs::remove_file(previous);
            }
        }
    }

    fn take(&mut self) -> Option<(String, PathBuf, u64)> {
        self.inner.take()
    }
}

impl Drop for StagedFile {
    fn drop(&mut self) {
        if let Some((_, path, _)) = &self.inner {
            let _ = std::fs::remove_file(path);
        }
    }
}

/// 配置了 token 时，上传与几何接口要求 `Authorization: Bearer <token>`。
fn check_token(state: &ApiState, headers: &HeaderMap) -> Result<(), String> {
    let Some(expected) = state.token.as_deref().filter(|token| !token.is_empty()) else {
        return Ok(());
    };
    let provided = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    // RFC 7235：scheme 不区分大小写；比较用恒定时间，避免计时侧信道。
    let (scheme, token) = provided.split_once(' ').unwrap_or((provided, ""));
    if scheme.eq_ignore_ascii_case("bearer") && constant_time_eq(token.trim(), expected) {
        return Ok(());
    }
    Err("缺少或错误的 Authorization: Bearer <token>。".to_string())
}

/// 逐字节比较，长度不同也走完整轮比较。
fn constant_time_eq(left: &str, right: &str) -> bool {
    let (left, right) = (left.as_bytes(), right.as_bytes());
    let mut diff = (left.len() ^ right.len()) != 0;
    for index in 0..left.len().max(right.len()) {
        let a = left.get(index).copied().unwrap_or(0);
        let b = right.get(index).copied().unwrap_or(0);
        diff |= a != b;
    }
    !diff
}

fn reject(status: StatusCode, message: String) -> Response {
    warn!("上传被拒绝：{message}");
    (status, Json(json!({ "error": message }))).into_response()
}
