// 本地浏览器预览用的最小 IPC mock；Tauri 环境不存在该对象时才启用。
window.__TAURI__ ??= {
  core: {
    invoke: async (command, args = {}) => {
      const screens = [
        {
          deviceId: "ZH-M1-4EBC5A23",
          name: "前门主屏",
          ip: "10.10.100.1",
          port: 20003,
          width: 1280,
          height: 128,
          rotate: 0,
          online: true,
        },
        {
          deviceId: "ZH-M1-8C21F04A",
          name: "大厅导视",
          ip: "10.10.100.12",
          port: 20003,
          width: 320,
          height: 240,
          rotate: 90,
          online: false,
        },
        {
          deviceId: "",
          name: "待确认屏幕",
          ip: "10.10.100.31",
          port: 20003,
          width: 0,
          height: 0,
          rotate: 0,
          online: true,
        },
      ];
      const jobs = [
        {
          jobId: "f28b1c7a92d54e318a6022bb7791dc4f",
          deviceId: "ZH-M1-4EBC5A23",
          fileName: "welcome.mp4",
          state: "Running",
          message: "校验通过，开始导出",
          progress: 58,
        },
        {
          jobId: "2ac41df809374d55a75c33d9d0e9cc61",
          deviceId: "ZH-M1-4EBC5A23",
          fileName: "notice.png",
          state: "Succeeded",
          message: "已发送",
        },
        {
          jobId: "9b0a2d5e6b824a9f9a9f5cdd7b28e6d0",
          deviceId: "ZH-M1-8C21F04A",
          fileName: "closed.gif",
          state: "Failed",
          error: "设备当前不在线，已中止发送。",
        },
      ];
      const logs = [
        "2026-11-19 18:20:04.128 [INF] upload_api : 已加载屏幕清单 appsettings.json，共 3 台屏幕。",
        "2026-11-19 18:20:08.315 [INF] sdk_host : HTTP 文件服务已启动于 0.0.0.0:18080。",
        "2026-11-19 18:22:17.734 [WRN] pipeline : 素材窗口 [0,0 2560x128] 超出屏幕 1280x128，已裁剪为 1280x128。",
        "2026-11-19 18:23:41.905 [ERR] jobs : 作业 9b0a2d5e：设备当前不在线，已中止发送。",
      ];
      switch (command) {
        case "list_screens":
          return screens;
        case "scan_screens":
          return { screens, scanned: 2, updated: 1 };
        case "get_status":
          return {
            sdkAvailable: true,
            sdk: "1.0.0 (ABI 2)",
            httpRunning: true,
            httpPort: 18080,
            uploadApiRunning: true,
            apiUrl: "http://127.0.0.1:5180",
            activeTaskCount: 2,
            searchTaskCount: 1,
            screens: screens.length,
            configPath: "D:\\Project\\LedSdk_windows_Tauri\\config\\appsettings.json",
            exportRoot: "D:\\Project\\LedSdk_windows_Tauri\\export",
            logDirectory: "D:\\Project\\LedSdk_windows_Tauri\\logs",
          };
        case "list_jobs":
          return jobs;
        case "read_logs":
          return logs;
        case "delete_screen":
          screens.splice(1, 1);
          return;
        case "update_screen":
          screens[1].name = args.name || screens[1].name;
          screens[1].ip = args.ip || screens[1].ip;
          screens[1].width = args.width ?? screens[1].width;
          screens[1].height = args.height ?? screens[1].height;
          screens[1].rotate = args.rotate ?? screens[1].rotate;
          return;
        default:
          return null;
      }
    },
  },
};
