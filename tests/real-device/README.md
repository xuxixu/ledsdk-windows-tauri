# 真机投屏回归素材

这三个素材用于验证「上传 JSON + 文件 → 建树 → 校验 → 导出 → 发送 → 真机 HTTP 拉包 → 上屏」的完整链路，于 2026-11-19 在真机 `ZH-M1-4EBC5A23`（10.10.100.1，屏幕 1280×128）上验证通过。

## 前置条件

1. 程序已启动，`GET /api/health` 返回 `httpRunning:true`、`sdkAvailable:true`。
2. 真机已上电并与本机同网段；点界面「扫描」后设备出现在清单里。
3. 补录几何：`POST /api/screens/{deviceId}/geometry`，body `{"width":1280,"height":128,"rotate":0}`。
4. 若需鉴权，给 curl 加 `-H "Authorization: Bearer <UploadApi.Token>"`（当前默认 Token 为空）。

## 用例

三个文件分别对应 `fileType` = `text` / `picture` / `video`，窗口均用整屏 `[0,0 1280×128]`。只改 `request.fileName` 与 `fileType` 以及 `-F file=@...`，其余一致：

```bash
# text.txt（UTF-8 两行中文）
curl -X POST http://127.0.0.1:5180/api/program \
  -F 'request={"fileName":"text.txt","fileType":"text","deviceId":"ZH-M1-4EBC5A23","x":0,"y":0,"width":1280,"height":128}' \
  -F 'file=@text.txt;type=text/plain'

# picture.png（1280×128 纯色）
curl -X POST http://127.0.0.1:5180/api/program \
  -F 'request={"fileName":"picture.png","fileType":"picture","deviceId":"ZH-M1-4EBC5A23","x":0,"y":0,"width":1280,"height":128}' \
  -F 'file=@picture.png;type=image/png'

# video.mp4（H.264/AAC，1280×128，3 秒）
curl -X POST http://127.0.0.1:5180/api/program \
  -F 'request={"fileName":"video.mp4","fileType":"video","deviceId":"ZH-M1-4EBC5A23","x":0,"y":0,"width":1280,"height":128}' \
  -F 'file=@video.mp4;type=video/mp4'
```

提交后用 `GET /api/jobs/{jobId}` 轮询到终态，断言：

- `state=Succeeded`、`message=已发送`、`progress=100`。
- 日志里出现 `send task N for device ... finished` 与 `file_state=5`（设备验收通过）。
- 日志里设备 `peer=10.10.100.1` 依次 HTTP 下载 `.zhl`（清单）、`.zhd`（节目）、`.zhp`（资源索引）及转码资源；视频资源名带目标几何 `..._ledsdk_1280x128.mp4`。

## 素材来源

用项目自带 ffmpeg 生成（换分辨率时改 `size`）：

```bash
ffmpeg -y -f lavfi -i "color=c=blue:size=1280x128" -frames:v 1 picture.png
ffmpeg -y -f lavfi -i "testsrc2=size=1280x128:rate=15:duration=3" \
  -f lavfi -i "sine=frequency=440:duration=3" \
  -c:v libx264 -pix_fmt yuv420p -c:a aac -shortest video.mp4
```

`text.txt` 为手写 UTF-8 两行中文，内容任意。
