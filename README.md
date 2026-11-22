# LedSdk_windows_Tauri

LED 控制卡节目上传服务，用 **Rust + Tauri 2** 重构自同目录的 C#/Avalonia 版本（`..\LedSdk_windows`）。

进程同时做三件事：

1. 按配置的 `Sdk.BinDirectory` 动态加载 LedSDK（校验 ABI 2），启动 SDK 的 HTTP 文件服务（默认 `0.0.0.0:18080`，根目录=导出目录）；
2. 常驻搜索设备（每 2 秒一轮 UDP 40000 广播，仅用于发现与发送前核对）；屏幕清单只在点「扫描」时写回配置文件；
3. 起 HTTP 上传接口（默认 `0.0.0.0:5180`）：收到「JSON + 素材文件」后建节目树 → 校验 → 导出 `.zhl` → 整文件 MD5 → `send_program` 发到指定屏幕（设备再回拉本机 HTTP 的节目包）。

Tauri 窗口是管理/监控面板：状态、屏幕清单（扫描/编辑/删除）、最近作业、运行日志，以及打开配置/导出/日志目录（只允许打开这四类目录，其它路径会被拒绝）。

- 托盘图标（`App.TrayIcon=true`，默认开启）：右键菜单「打开界面 / 退出」，左键单击/双击也可重新显示窗口；关闭窗口时隐藏到托盘，程序继续提供上传接口与设备发现。设为 `false` 时不创建托盘，关闭窗口即退出。
- 桌面图标（`App.DesktopShortcut=true`，默认开启）：每次启动同步用户桌面 `LedSdk.lnk`，指向当前 exe；设为 `false` 时删除该快捷方式。安装程序还可单独选择是否创建桌面/开始菜单图标。

## 目录结构

```
LedSdk_windows_Tauri/
├─ Cargo.toml / build.rs / tauri.conf.json
├─ config/appsettings.json     配置（build.rs 会复制到 <target>/<profile>/config/）
├─ sdk/
│  ├─ ledsdk.h                 原生接口头文件（唯一权威）
│  ├─ x64/                     Windows：LedSDK.dll、LedSDK.lib、FFmpeg、Qt5 DLL、LedSDKTextRenderer.exe、platforms/
│  └─ linux-x64/               Linux：libLedSDK.so{,.1,.1.0.0}、LedSDKTextRenderer、ledsdk.h
├─ src/
│  ├─ main.rs                  Tauri 装配、窗口命令、--selfcheck/--simulate 入口
│  ├─ ffi.rs                   头文件对应的结构体/常量、动态加载的 SDK 函数表
│  ├─ host.rs                  SdkHost：初始化/搜索快照/导出/发送/关闭顺序
│  ├─ pipeline.rs              单个上传作业的全流程（建树→校验→导出→校验包→发送）
│  ├─ jobs.rs                  单消费者作业队列 + 作业表
│  ├─ server.rs                axum 上传接口
│  ├─ config.rs                appsettings 读写、屏幕清单（扫描/编辑/删除后原子写回）
│  ├─ retention.rs             按有效天数清理日志与导出产物
│  ├─ winos.rs                 开机自启（HKCU Run）、作业对象（子进程随主进程退出）
│  ├─ runtime.rs               启动/关闭编排、日志尾部读取
│  ├─ selfcheck.rs / simulate.rs  无窗口自检与模拟测试
│  ├─ logging.rs / models.rs / net.rs
└─ ui/                         index.html / style.css / main.js（静态前端，不需要 Node）
```

## 环境要求

构建环境：

- Rust 1.80+（本机验证 1.99.1），目标 `x86_64-pc-windows-msvc`；
- Visual Studio 生成工具（C++ x64 工具链，链接 `LedSDK.lib` 需要）；
- WebView2 Runtime（Windows 10/11 一般已内置）；
- 不需要 Node、npm、tauri-cli：`ui/` 是静态文件，`bundle.active=false`，`cargo build` 直接产出 exe。

目标机要求（发行版 `dist/`）：

| 项 | 要求 | 依据 |
| --- | --- | --- |
| 操作系统 | Windows 10 1809+ / Windows 11，x64 | exe 导入 `combase.dll`、`api-ms-win-core-winrt-error-l1-1-0.dll`（Windows 8+），且 WebView2 的新版本已不支持 Win7/8.1 |
| 架构 | 仅 x64 | `LedSDK.dll`、Qt5、FFmpeg 全是 x64，不能与 32 位混用 |
| WebView2 Runtime | 必需 | 界面是 WebView2；Win10/11 通常随 Edge 已装（本机 157.x） |
| VC++ 2013 x64 运行库 | **文本导出必需** | `LedSDKTextRenderer.exe`、`Qt5Core/Gui/Widgets.dll`、`platforms\qwindows.dll` 依赖 `MSVCP120.dll` / `MSVCR120.dll`；发行包已带 `vcredist_x64.exe`，没装过的机器先运行一次 |
| 端口 | TCP 18080 入站放行 | 设备回拉节目包（`.zhl`/`.zhd`/`.zhp`/资源） |
| 权限 | 普通用户即可 | 开机自启只写 `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` |
| 网络 | 与屏幕同网段（或路由可达） | 发送前按 `deviceId` 核对在线，下载地址取「设备回包里命中的本机网卡」 |

## 配置

`config/appsettings.json`（UTF-8，PascalCase；相对路径以**配置文件所在目录**为基准，所以默认值用 `../export` 这类写法，让数据目录落在 exe 旁边）：

| 键 | 默认 | 说明 |
| --- | --- | --- |
| `Sdk.BinDirectory` | `..` | LedSDK 动态库所在目录；运行时由 `libloading` 按该路径加载。`..` 指配置文件上一级 = exe 目录（build.rs 已把 `sdk/x64` 的文件复制到那里） |
| `Sdk.BindAddress` | `0.0.0.0` | SDK HTTP 服务绑定地址 |
| `Sdk.DownloadPort` | `18080` | 设备回拉节目包用的端口 |
| `Sdk.ExportRoot` | `../export` | 导出目录，同时是 HTTP 下载根目录 |
| `Sdk.UploadRoot` | `../upload` | 上传暂存的落盘目录（作业结束即删） |
| `Sdk.TranscodeCacheRoot` | `../cache` | 视频/GIF 转码缓存 |

> 18080 只用于设备回拉节目包。SDK 的 HTTP 服务本身没有“关闭上传”开关，程序启动后会删除其独立的
> `.sdk-http-incoming` 目录，使 POST/PUT 返回 400；同时不启用 `ALLOW_ABSOLUTE_PATH`，无法通过
> `/d:/...` 读取下载根目录之外的文件。不要手工重建该目录，也不要把 `ExportRoot`/`UploadRoot` 配成同一目录。
| `Sdk.WorkerCount` | `2` | SDK 工作线程数 |
| `Sdk.DefaultDurationSeconds` / `DefaultRepeatTimes` | `5` / `1` | 文本与图片素材的默认播放时长/次数 |
| `Sdk.SendRetryCount` / `SendRetryBackoffMs` | `2` / `500` | 发送重试 |
| `Sdk.ExportOnly` | `false` | `true` 时只建树/校验/导出，不发送（离线联调用） |
| `UploadApi.ListenAddress` / `Port` | `0.0.0.0` / `5180` | 上传接口监听 |
| `UploadApi.Token` | `""` | 非空时，上传与几何接口要求 `Authorization: Bearer <token>`；留空不校验（仅限受控网络） |
| `App.AutoStart` | `false` | 开机自启（写 HKCU `Run`，值名 `LedSdkTauri`）：`true` 写入/校正，`false` 删除；每次启动时同步，改配置后需重启生效 |
| `App.TrayIcon` | `true` | 显示系统托盘图标；关闭窗口时隐藏到托盘。`false` 时关闭窗口直接退出 |
| `App.DesktopShortcut` | `true` | 在用户桌面创建/校正 `LedSdk.lnk`；`false` 时删除该快捷方式 |
| `App.RetentionDays` | `30` | 启动时清理超过该天数的日志、导出产物与 `upload/` 里的残留文件（`<=0` 不清理） |
| `Screens[]` | `[]` | 屏幕清单，字段见下；搜索发现会按 `DeviceId` upsert 并写回 |

`Screens` 单项：`Name`、`DeviceId`、`Ip`、`Port`（默认 20003）、`DeviceType`、`Width`、`Height`、`Rotate`、`LocalAddress`、`LastSeen`、`Online`、`Source`（`manual` / `discovered`）。

- 自动字段（`DeviceId`/`Ip`/`Port`/`DeviceType`/`LocalAddress`/`Online`/`LastSeen`）由搜索刷新；
- 人工字段（`Width`/`Height`/`Rotate`）永不覆盖，因为搜索不提供分辨率。

## 构建与运行

```powershell
cd D:\Project\LedSdk_windows_Tauri
cargo build                 # 产物 target\debug\ledsdk-tauri.exe
cargo build --release       # 产物 target\release\ledsdk-tauri.exe
.\target\debug\ledsdk-tauri.exe
```

无窗口自检 / 模拟测试（不依赖数据库、不需要真实设备）：

```powershell
.\target\debug\ledsdk-tauri.exe --selfcheck   # 结构体尺寸 + ABI 2 + 清单/备份恢复 + 节目包校验 + 异常日志 + 保留清理 + SDK HTTP 隔离 + 启停生命周期
.\target\debug\ledsdk-tauri.exe --simulate    # 3 台屏幕 × 文字/图片/视频/GIF + 大文件上传 + 无 token 401 + 作业中关闭（仅 debug 版）
```

`--simulate` 用 `sdk/x64/ffmpeg.exe` 现场生成素材，产物落在 `%TEMP%\ledsdk-tauri-simulate\run-<epoch>`；其中一张 PNG 故意大于 2 MB，用来守住「大文件上传不被请求体上限挡掉」。模拟测试依赖素材与 ffmpeg，只编进 debug 版；`--selfcheck` 保留在发行版里，部署机可以直接用它验证。发行版执行 `--simulate` 会以退出码 2 结束。

> `--selfcheck` / `--simulate` 与启动失败路径用 `TerminateProcess` 退出：LedSDK.dll 是静态导入，
> 若从未初始化过，正常退出会在它的 C++ 静态析构里抛异常（表现为 0xC0000409）。

## Linux 构建（功能与 Windows 版一致）

厂商提供了 Linux x64 SDK（`libLedSDK.so` + `LedSDKTextRenderer`），本项目已内置到 `sdk/linux-x64/`，
运行时按 `Sdk.BinDirectory` 动态加载，行为与 Windows 版一致（建树 / 校验 / 导出 / 发送全部可用）。

```bash
# SDK 运行依赖：Qt6 运行库（文本渲染器）、FFmpeg（转码与探测）、中文字体
sudo apt install -y ffmpeg libqt6core6 libqt6gui6 libqt6widgets6 qt6-qpa-plugins fonts-noto-cjk

# 编译依赖：Rust + Tauri 2 的系统库
sudo apt install -y build-essential curl pkg-config libwebkit2gtk-4.1-dev libgtk-3-dev \
    libayatana-appindicator3-dev librsvg2-dev

cargo build --release
./target/release/ledsdk-tauri          # 界面 + HTTP 接口
./target/release/ledsdk-tauri --selfcheck
```

> 仓库里带了 `build-linux-verify.sh`，可在 WSL 里一键完成「装 Rust → 编译 → 自检」：
> `wsl.exe -u root -e bash /mnt/d/Project/LedSdk_windows_Tauri/build-linux-verify.sh`

`target/release/` 里除了可执行文件，还会有 `libLedSDK.so{,.1,.1.0.0}`、`LedSDKTextRenderer` 和 `config/appsettings.json`：
`build.rs` 负责把 SO 复制到目标目录；运行时通过 `libloading` 按 `Sdk.BinDirectory` 的绝对路径加载，`LedSDKTextRenderer` 必须与它同目录（SDK 的要求）。

> 从 Windows 拷贝过来的仓库会丢掉执行位：`build.rs` 复制到 `target/` 时会为 `LedSDKTextRenderer` 补 `755`；
> 如果直接在仓库目录里运行，先 `chmod +x sdk/linux-x64/LedSDKTextRenderer`。

与 Windows 的差异：

| 项目 | Windows | Linux |
| --- | --- | --- |
| SDK | `LedSDK.dll` / `LedSDK.lib`（`sdk/x64/`） | `libLedSDK.so` / `libLedSDK.so.1`（`sdk/linux-x64/`） |
| 文本渲染 | `LedSDKTextRenderer.exe` + Qt5 | `LedSDKTextRenderer` + 系统 Qt6 |
| FFmpeg | 包内 `ffmpeg.exe` / `ffprobe.exe` | 系统 `ffmpeg` / `ffprobe`（或 `LEDSDK_FFMPEG_PATH` / `LEDSDK_FFPROBE_PATH`） |
| 子进程兜底 | Windows 作业对象（崩溃连带结束 ffmpeg） | 无（依赖正常退出时取消任务） |
| 开机自启 | HKCU Run 注册表 | 未实现（建议 systemd user unit） |

`src/ffi.rs` 的 `sdk_api!` 宏在 Windows / Linux 上展开成动态加载的函数指针调用，
其它平台展开成返回 `NOT_SUPPORTED(-11)` 的桩；`SdkHost::available()` 只在 Windows / Linux 返回 true，
其它平台 `GET /api/health` 报 `sdkAvailable: false`、`POST /api/program` 返回 503。

> 已用 C 探针在 Debian x64 上验证过：`libLedSDK.so` 可加载、`ledsdk_get_api_version` 返回 1.0.0 / ABI 2、
> 14 个结构体尺寸与 Rust 侧一致、`initialize` / `get_runtime_status` / `shutdown` 正常。

## macOS（仅界面骨架，不能上屏）

厂商没有提供 macOS 版 SDK（只有 Windows / Linux），所以 macOS 构建只能启动界面与 HTTP 查询接口，
不能建树 / 导出 / 发送。表现：

- `GET /api/health` 返回 `sdkAvailable: false`；界面状态行显示「SDK 不可用（本平台没有 LedSDK）」。
- `POST /api/program` 返回 503，并说明原因；几何接口不依赖 SDK，仍可补录。
- `--selfcheck` 跳过 SDK 版本与生命周期检查；`--simulate` 打印提示后退出（macOS 无 SDK）。
- 「打开目录」按钮在 macOS 用 `open` 而不是 Linux 的 `xdg-open`。

编译依赖（在 macOS 本机）：

```bash
xcode-select --install
cargo build --release
./target/release/ledsdk-tauri          # 界面 + HTTP 查询接口
./target/release/ledsdk-tauri --selfcheck
```

> `tauri.conf.json` 的 bundle 图标含 `icons/icon.icns`；`bundle.active=false`，不产出 .dmg/.app 安装包。
> 要让 macOS 真正上屏，需厂商提供 macOS 版动态库（如 `libLedSDK.dylib`）后再扩展 `sdk_api!` 与 `build.rs`。

## 上传接口

`POST /api/program`，`multipart/form-data`，两个字段：

- `request`：JSON 字符串
- `file`：素材文件（文件名与 `fileName` 不一致时以实际上传的文件为准，并记 WARN）

```json
{
  "fileName": "picture.png",
  "fileType": "picture",
  "deviceId": "ZH-M1-4EBC5A23",
  "x": 0,
  "y": 0,
  "width": 1280,
  "height": 128
}
```

- `fileType`：`text` / `picture`（同 `image`）/ `video` / `gif`；
- `x`、`y`、`width`、`height` 是素材在屏幕上的窗口；超出屏幕会被自动裁剪并记 WARN，起点越界直接 400；
- `deviceId` 必须出现在 `Screens` 里（`GET /api/screens` 可查）且有几何，否则 400；设备不在当前搜索快照里也会被拦下（不会把崩溃交给内核态）。

```powershell
curl.exe -X POST http://127.0.0.1:5180/api/program `
  -F 'request={"fileName":"picture.png","fileType":"picture","deviceId":"ZH-M1-4EBC5A23","x":0,"y":0,"width":640,"height":128}' `
  -F 'file=@D:\tmp\picture.png'
```

其它接口：

| 方法 | 路径 | 说明 |
| --- | --- | --- |
| GET | `/api/health` | SDK 版本、HTTP 状态、任务计数、屏幕数 |
| GET | `/api/jobs?limit=20` | 最近作业 |
| GET | `/api/jobs/{jobId}` | 单个作业状态 |
| GET | `/api/screens` | 屏幕清单 |
| POST | `/api/screens/{deviceId}/geometry` | 补录几何：`{"width":1280,"height":128,"rotate":0}` |

作业是异步的：上传返回 `202` + 首个状态，之后轮询 `/api/jobs/{jobId}` 到 `Succeeded` / `Failed`。

- 请求体上限 1 GB（`DefaultBodyLimit`，`Content-Length` 超限会提前 413）。素材是边收边写磁盘的，大文件不会整块进内存；作业失败或请求被拒时半截文件会被清掉。
- `request` 字段（JSON）上限 64 KB，超过返回 400「request 字段过大」。一次请求带多个 file 字段时只保留最后一个，先落盘的那个会被删掉。
- 等待执行的作业上限 64 个；队列满时上传接口返回失败状态并清理暂存文件，避免无限堆积。
- 配了 `UploadApi.Token` 时，`POST /api/program` 与 `POST /api/screens/{deviceId}/geometry` 需要 `Authorization: Bearer <token>`，缺失返回 401。
- Swagger：`http://127.0.0.1:5180/swagger-ui/#/`（OpenAPI JSON 在 `/api-docs/openapi.json`）。
- 窗口里配了 CSP（`tauri.conf.json` → `app.security.csp`）。注意 `script-src` 必须带 `'unsafe-inline'`：Tauri 会往页面注入内联的 IPC 引导脚本，去掉它窗口会变成极小尺寸且不渲染。`connect-src` 必须保留 `ipc: http://ipc.localhost`，否则 `invoke` 全部失败。

## 日志

- 位置：`<exe目录>/logs/<yyyy-MM-dd>/`（每天一个目录，UTF-8）；主日志 `app.log`，异常日志 `errors.log`；
- 从旧版升级：若 `logs/<yyyy-MM-dd>` 已存在同名**文件**（旧版命名），程序不再建目录，改为写 `logs/app-<yyyy-MM-dd>.log`（异常同理 `errors-<yyyy-MM-dd>.log`），不会静默丢日志；想恢复目录布局，把那个旧文件改名即可。
- 行格式：`yyyy-MM-dd HH:mm:ss.fff [INF] target : message`；
- SDK 自身日志以 `[SDK]` 前缀汇入同一文件；
- 记录入参、分支决策、异常与任务终态；不记每次进度回调、搜索每轮广播、密码。
- **异常单独写 `logs/<yyyy-MM-dd>/errors.log`**：全局 panic hook 抓到 Rust 侧任何线程的未捕获异常（`[PANIC]`，带线程名、位置和完整调用栈），前端 `window.onerror` / `unhandledrejection` 抓到的异常记 `[UI]`（带页面位置）。两者都不进主日志，避免被日常输出冲散；同一条异常也会在界面的「运行日志」里补一条 WRN。异常文件与普通日志一样受 `App.RetentionDays` 清理。

## 排错

| 现象 | 处理 |
| --- | --- |
| 发送超时、设备没开始下载 | 1) 防火墙放行本程序 TCP 18080 入站；2) 下载地址必须是设备同网段的本机 IPv4；3) HTTP 根目录与导出目录一致 |
| `设备 '…' 不在当前搜索快照中` | 设备离线/不在同网段；确认上电后等一轮搜索（2 秒）再重试 |
| `本机有 N 个候选网卡` | 多网卡无法确定设备可达地址：给该屏幕配 `LocalAddress`，或让设备与某一网段同网段 |
| 设备拒收节目（`file_state=3`） | 屏幕 `Width`/`Height` 与面板实际分辨率不符（搜索不提供分辨率，需人工补录） |
| 界面空白 | 装 WebView2 Runtime；看 `<exe>/logs/<当天>/app.log` 与 `errors.log` |
| 程序意外退出 | 先看 `logs/<当天>/errors.log`：`[PANIC]` 段带线程、位置与调用栈 |

## 与 C# 版的对应

| C#/Avalonia | 本仓库 |
| --- | --- |
| `SdkHost.cs` | `src/host.rs`（关闭顺序、任务终态后才释放句柄、`task_cancel_all` 3 秒上限） |
| `LedJobProcessor.cs` | `src/pipeline.rs` + `src/jobs.rs` |
| `UploadEndpoints` + Kestrel | `src/server.rs`（axum） |
| `ScreenRegistry.cs` | `src/config.rs`（扫描/编辑/删除后原子写回） |
| `RetentionCleaner.cs` | `src/retention.rs` |
| `AutoStartManager.cs` / `ProcessJob.cs` | `src/winos.rs`（HKCU Run + 作业对象 `KILL_ON_JOB_CLOSE`） |
| `MainViewModel.cs` + XAML | `ui/`（静态 HTML/JS，`invoke` 取数据） |
| selfcheck / simulate | `src/selfcheck.rs` / `src/simulate.rs`（`--selfcheck` 随发行版；`--simulate` 仅 debug 版） |

## 发版

```powershell
.\dist.ps1     # cargo build --release + 组装 dist\，并生成 dist\LedSdk-Setup-<version>.exe
```

`dist.ps1` 不清理 `dist\` 里的旧文件（删除需人工确认）；已有的 `dist\config\appsettings.json` 不会被覆盖，模板另存为 `appsettings.template.json`。上游文件改名后请手动删残留。

### 单文件安装包（推荐）

`build-installer.ps1` 使用 Windows 自带的 .NET Framework 4.8 `csc.exe`，把 `dist\` 的运行文件压成内嵌负载，生成单文件 `LedSdk-Setup-<version>.exe`，不引入第三方安装器依赖：

```powershell
.\build-installer.ps1                          # 单独生成安装包
.\dist\LedSdk-Setup-1.0.1.exe                  # 图形安装（可改安装路径、勾选桌面/开始菜单图标）
.\dist\LedSdk-Setup-1.0.1.exe /silent          # 静默安装（默认创建两个图标）
.\dist\LedSdk-Setup-1.0.1.exe /silent /nodesktop /nostartmenu
.\dist\LedSdk-Setup-1.0.1.exe /silent /dir D:\Apps\LedSdk
.\dist\LedSdk-Setup-1.0.1.exe /verify <目录>   # 只校验内嵌负载，不安装
```

图形安装可在「安装位置」中直接输入或点击「浏览…」选择目录；升级时会优先填入注册表中记录的现有安装位置。默认安装目标为 `%LOCALAPPDATA%\Programs\LedSdk`，静默安装可用 `/dir <绝对路径>` 指定目录。安装器会拒绝磁盘根目录、Windows 系统目录、被文件占用的路径和不可写目录。安装后会写入 HKCU「应用和功能」卸载项，卸载时只删除指向本安装目录的快捷方式与自启项。托盘模式下关闭窗口或点「退出」都会先询问是否退出到托盘模式；选「是」隐藏到托盘，选「否」关闭软件。再次启动会自动唤起已有窗口，不会重复占用 SDK 端口。

卸载使用安装目录下的 `uninstall.exe`（或 Windows「应用和功能」中的 LedSdk）。卸载窗口会完整显示自定义安装路径；卸载前会检查该目录下的程序是否正在运行，如正在运行可选择强制停止；可勾选删除桌面/开始菜单快捷方式、开机自启动，以及是否保留用户数据（配置/导出/日志/缓存/上传）。安装和卸载界面均使用自适应布局，长标题、长路径和状态文字不会被截断或相互遮挡。

### 便携/脚本安装（可选）

`installer\setup.ps1` 仍可单独使用，适合复制 `dist\` 到目标机后运行：

```powershell
.\setup.ps1                          # 交互式，询问是否创建桌面/开始菜单图标
.\setup.ps1 -NoDesktopShortcut       # 不创建桌面图标
.\setup.ps1 -NoStartMenuShortcut     # 不创建开始菜单图标
.\setup.ps1 -Quiet                   # 静默安装，默认两个图标都创建
.\setup.ps1 -Uninstall               # 卸载（先检查并确认是否强制停止运行中的实例）
.\setup.ps1 -TargetDir D:\Apps\LedSdk  # 安装到指定绝对目录
.\setup.ps1 -Uninstall -TargetDir D:\Apps\LedSdk -PurgeData  # 卸载并删除用户数据
```

不指定 `-TargetDir` 时安装目标为 `%LOCALAPPDATA%\Programs\LedSdk`。脚本会同步写入 `App.TrayIcon=true` 和 `App.DesktopShortcut`，并按 `App.AutoStart` 维护 HKCU Run 项；运行中进程检查只匹配当前目标目录下的 `ledsdk-tauri.exe`。

## 验证状态

| 项 | 结果 |
| --- | --- |
| `cargo build` / `cargo build --release` | 0 警告 0 错误 |
| `--selfcheck` | 14/14 结构体尺寸、`LedSDK 1.0.0 (ABI 2)`、清单编辑/坏配置恢复、节目包校验、异常日志、保留清理、SDK HTTP 隔离、生命周期通过 |
| `--simulate` | 85 项断言通过（3 屏 × 文字/单行文字/图片/视频/GIF + 4.1 MB 大文件上传 + 无 token/错 token 401 + 作业中关闭） |
| 上传上限 | 3 MB / 4.1 MB 上传均正常（旧实现被 axum 默认 2 MB 挡掉） |
| 接口鉴权 | `UploadApi.Token` 非空时缺 `Authorization` 返回 401 |
| 桌面窗口 | 标题栏正常、状态/屏幕/作业/日志四块渲染正常（CSP 开启后复核） |
| 关闭 | 确认后 0.2 秒退出，无残留进程，18080/5180 释放 |
| 退出按钮/右上角关闭 | 均弹出「是否退出到托盘模式」；是→隐藏，否→退出并释放端口 |
| 单实例 | 关闭到托盘后再次启动，新进程约 0.2 秒退出并唤起已有窗口 |
| 启动失败 / CLI 早退 | 干净返回退出码（不再出现 0xC0000409 外来异常中止） |
| 单文件安装包 | `dist\LedSdk-Setup-1.0.1.exe` 图形/静默安装、卸载、内嵌负载校验通过 |
| 卸载程序 | 运行检测/强制停止、快捷方式/自启/用户数据勾选、卸载项清理通过 |
| 真机投屏 | 未在本次环境验证（需设备同网段） |
