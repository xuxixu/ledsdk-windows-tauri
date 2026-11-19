// 界面：左「屏幕清单」（手动扫描/重载/编辑/删除），右「最近作业 + 运行日志」。
// 屏幕清单不再自动刷新——只有扫描、重载、编辑、删除后重新读取。
const invoke = window.__TAURI__.core.invoke;

// 前端未捕获异常也进 errors 日志（对应 Rust 侧的全局 panic hook）。
// 注意别用 invoke(...).catch 之外的 await，避免监听器本身抛错。
window.addEventListener("error", (event) => {
  invoke("log_ui_error", {
    message: String(event.message || event.error || "未知错误"),
    source: `${event.filename || ""}:${event.lineno || 0}:${event.colno || 0}`,
  }).catch(() => {});
});
window.addEventListener("unhandledrejection", (event) => {
  invoke("log_ui_error", {
    message: String(event.reason || "未处理的 Promise 拒绝"),
    source: "unhandledrejection",
  }).catch(() => {});
});

let configDirectory = "";
let exportRoot = "";
let logDirectory = "";

let screens = [];
let selectedIndex = -1;

const escapeHtml = (value) =>
  String(value ?? "").replace(/[&<>"']/g, (ch) => ({
    "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;",
  })[ch]);

function cell(text, className) {
  return `<td${className ? ` class="${className}"` : ""}>${escapeHtml(text)}</td>`;
}

function setHint(text) {
  document.getElementById("screen-hint").textContent = text;
}

function rowKey(screen) {
  return `${screen.deviceId}||${screen.name}`;
}

function selectedScreen() {
  return selectedIndex >= 0 && selectedIndex < screens.length ? screens[selectedIndex] : null;
}

function renderScreens() {
  document.getElementById("screen-count").textContent = `（${screens.length} 台）`;
  const body = document.getElementById("screens");
  if (!screens.length) {
    body.innerHTML = `<tr><td colspan="5">还没有屏幕：点「扫描」发现设备，或「重新加载配置」读回手工条目。</td></tr>`;
    return;
  }
  body.innerHTML = screens
    .map((screen, index) => {
      const geometry =
        screen.width > 0 && screen.height > 0 ? `${screen.width}x${screen.height}` : "（未补录）";
      const rowClass = index === selectedIndex ? ' class="selected"' : "";
      return `<tr data-index="${index}"${rowClass}>${
        cell(screen.deviceId || "（无 deviceId）")
      }${cell(screen.name)}${cell(`${screen.ip}:${screen.port}`)}${cell(geometry)}${
        cell(screen.online ? "在线" : "离线")
      }</tr>`;
    })
    .join("");
}

async function loadScreens() {
  screens = await invoke("list_screens");
  if (selectedIndex >= screens.length) selectedIndex = -1;
  renderScreens();
}

document.getElementById("screens").addEventListener("click", (event) => {
  const row = event.target.closest("tr[data-index]");
  if (!row) return;
  selectedIndex = Number(row.dataset.index);
  renderScreens();
  const screen = selectedScreen();
  if (screen) setHint(`已选中 ${screen.deviceId || screen.name}。`);
});

document.getElementById("scan").addEventListener("click", async (event) => {
  const button = event.currentTarget;
  if (button.disabled) return;
  button.disabled = true;
  setHint("正在扫描网段内的设备…");
  try {
    const result = await invoke("scan_screens");
    screens = result.screens;
    selectedIndex = -1;
    renderScreens();
    setHint(`扫描完成：搜索到 ${result.scanned} 台设备，${result.updated} 条清单更新（已写回配置）。`);
  } catch (err) {
    setHint(`扫描失败：${err}`);
  } finally {
    button.disabled = false;
  }
});

document.getElementById("reload").addEventListener("click", async (event) => {
  const button = event.currentTarget;
  if (button.disabled) return;
  button.disabled = true;
  try {
    const result = await invoke("reload_screens");
    screens = result.screens;
    selectedIndex = -1;
    renderScreens();
    setHint(`已重新加载配置：${screens.length} 台屏幕。`);
  } catch (err) {
    setHint(`重新加载失败：${err}`);
  } finally {
    button.disabled = false;
  }
});

document.getElementById("delete").addEventListener("click", async () => {
  const screen = selectedScreen();
  if (!screen) return setHint("先选中一行再删除。");
  const label = screen.deviceId || screen.name;
  if (!window.confirm(`删除屏幕 ${label}？会立即写回配置文件。`)) return;
  try {
    await invoke("delete_screen", {
      deviceId: screen.deviceId,
      name: screen.name,
    });
    // 删除后索引会指向另一台屏幕：必须清空选中，否则「编辑/删除」会误作用到别的行。
    selectedIndex = -1;
    await loadScreens();
    setHint(`已删除 ${label}。`);
  } catch (err) {
    setHint(`删除失败：${err}`);
  }
});

const dialog = document.getElementById("edit-dialog");

document.getElementById("edit").addEventListener("click", () => {
  const screen = selectedScreen();
  if (!screen) return setHint("先选中一行再编辑。");
  document.getElementById("f-device").value = screen.deviceId;
  document.getElementById("f-name").value = screen.name;
  document.getElementById("f-ip").value = screen.ip;
  document.getElementById("f-port").value = screen.port;
  document.getElementById("f-width").value = screen.width;
  document.getElementById("f-height").value = screen.height;
  document.getElementById("f-rotate").value = screen.rotate;
  dialog.showModal();
});

dialog.addEventListener("close", async () => {
  if (dialog.returnValue !== "save") return;
  const screen = selectedScreen();
  if (!screen) return;
  const name = document.getElementById("f-name").value.trim();
  const ip = document.getElementById("f-ip").value.trim();
  const port = Number(document.getElementById("f-port").value);
  const width = Number(document.getElementById("f-width").value);
  const height = Number(document.getElementById("f-height").value);
  const rotate = Number(document.getElementById("f-rotate").value);
  if (!name || !ip) return setHint("名称和 IP 不能为空。");
  if (!(port >= 1 && port <= 65535)) return setHint("端口必须在 1-65535 之间。");
  if (!(width > 0 && height > 0)) return setHint("宽度/高度必须大于 0。");
  if (![0, 90, 180, 270].includes(rotate)) return setHint("旋转只能是 0/90/180/270。");
  try {
    await invoke("update_screen", {
      deviceId: screen.deviceId,
      name,
      ip,
      port,
      width,
      height,
      rotate,
    });
    await loadScreens();
    setHint(`已保存 ${screen.deviceId || name}。`);
  } catch (err) {
    setHint(`保存失败：${err}`);
  }
});

async function refreshLive() {
  try {
    const status = await invoke("get_status");
    configDirectory = parentOf(status.configPath);
    exportRoot = status.exportRoot;
    logDirectory = status.logDirectory;

    const http = status.httpRunning ? `HTTP ${status.httpPort} 运行中` : "HTTP 未运行";
    const active = status.activeTaskCount - status.searchTaskCount;
    document.getElementById("status").textContent =
      `SDK ${status.sdkAvailable === false ? "不可用（本平台没有 LedSDK）" : status.sdk}　|　${http}　|　上传接口 ${status.apiUrl}　|　` +
      `屏幕 ${status.screens} 台　|　进行中作业 ${active}（搜索常驻 ${status.searchTaskCount}）`;

    const jobs = await invoke("list_jobs", { limit: 20 });
    document.getElementById("job-count").textContent = `（最近 ${jobs.length} 条）`;
    document.getElementById("jobs").innerHTML = jobs.length
      ? jobs.map((job) => `<tr>${
          cell(job.jobId.slice(0, 12))
        }${cell(job.deviceId)}${cell(job.fileName)}${cell(job.state, `state-${job.state}`)}${
          cell(job.error || job.message, "wrap")
        }</tr>`).join("")
      : `<tr><td colspan="5">还没有作业：POST /api/program 提交一次上传。</td></tr>`;

    const logs = await invoke("read_logs", { lines: 200 });
    const box = document.getElementById("logs");
    box.textContent = logs.length ? logs.join("\n") : "（今天还没有日志）";
    box.scrollTop = box.scrollHeight;
  } catch (err) {
    document.getElementById("status").textContent = `读取状态失败：${err}`;
  }
}

function parentOf(path) {
  if (!path) return "";
  const index = Math.max(path.lastIndexOf("\\"), path.lastIndexOf("/"));
  return index > 0 ? path.slice(0, index) : path;
}

document.getElementById("open-config").addEventListener("click", () => {
  if (configDirectory) invoke("open_folder", { path: configDirectory });
});
document.getElementById("open-export").addEventListener("click", () => {
  if (exportRoot) invoke("open_folder", { path: exportRoot });
});
document.getElementById("open-logs").addEventListener("click", () => {
  if (logDirectory) invoke("open_folder", { path: logDirectory });
});
document.getElementById("quit").addEventListener("click", () => invoke("quit_app"));

loadScreens();
refreshLive();
setInterval(refreshLive, 2000);
