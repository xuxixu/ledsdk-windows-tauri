// LedSdk 单文件安装程序：内嵌 dist 发行负载，复用 setup.ps1 完成安装。
// 目标框架为 Windows 自带的 .NET Framework 4.8，构建时调用系统 csc.exe，不引入第三方依赖。
using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Drawing;
using System.IO;
using System.IO.Compression;
using System.Reflection;
using System.Runtime.InteropServices;
using System.Text;
using System.Threading;
using System.Windows.Forms;
using Microsoft.Win32;

[assembly: AssemblyTitle("LedSdk 安装程序")]
[assembly: AssemblyDescription("LedSdk 屏幕节目上传服务安装程序")]
[assembly: AssemblyProduct("LedSdk")]
[assembly: AssemblyCompany("LedSdk")]
[assembly: AssemblyVersion("1.0.4.0")]
[assembly: AssemblyFileVersion("1.0.4.0")]

namespace LedSdkSetup
{
    internal sealed class InstallOptions
    {
        public string TargetDir = "";
        public bool DesktopShortcut = true;
        public bool StartMenuShortcut = true;
        public bool LaunchAfterInstall = true;
    }

    internal static class Program
    {
        internal const string Version = "1.0.4";
        internal const string PayloadResource = "LedSdkPayload.zip";
        private const string WebView2DownloadUrl = "https://go.microsoft.com/fwlink/p/?LinkId=2124703";

        [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
        private struct OSVERSIONINFOEX
        {
            public int dwOSVersionInfoSize;
            public int dwMajorVersion;
            public int dwMinorVersion;
            public int dwBuildNumber;
            public int dwPlatformId;
            [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 128)]
            public string szCSDVersion;
            public ushort wServicePackMajor;
            public ushort wServicePackMinor;
            public ushort wSuiteMask;
            public byte wProductType;
            public byte wReserved;
        }

        [DllImport("user32.dll")]
        private static extern bool SetProcessDPIAware();

        [DllImport("ntdll.dll", CharSet = CharSet.Unicode)]
        private static extern int RtlGetVersion(ref OSVERSIONINFOEX versionInfo);

        [STAThread]
        private static int Main(string[] args)
        {
            try { SetProcessDPIAware(); } catch { }
            Application.EnableVisualStyles();
            Application.SetCompatibleTextRenderingDefault(false);

            bool verify = HasArg(args, "/verify");
            bool uninstall = HasArg(args, "/uninstall");
            bool silent = HasArg(args, "/silent");
            if (verify)
            {
                return VerifyPayload(ArgValue(args, "/verify"));
            }
            if (uninstall)
            {
                return RunUninstall();
            }

            string preflight = Preflight();
            if (preflight != null)
            {
                if (silent)
                {
                    string log = Path.Combine(Path.GetTempPath(), "LedSdkSetup.log");
                    try { File.WriteAllText(log, ""); } catch { }
                    Log(log, "安装前检查失败：" + preflight);
                    return 1;
                }
                MessageBox.Show(preflight, "无法安装 LedSdk", MessageBoxButtons.OK, MessageBoxIcon.Warning);
                return 1;
            }
            if (!HasWebView2())
            {
                if (silent)
                {
                    string log = Path.Combine(Path.GetTempPath(), "LedSdkSetup.log");
                    try { File.WriteAllText(log, ""); } catch { }
                    Log(log, "缺少 Microsoft Edge WebView2 Runtime，请先安装：" + WebView2DownloadUrl);
                    return 1;
                }
                DialogResult open = MessageBox.Show(
                    "未检测到 Microsoft Edge WebView2 Runtime。\r\n\r\n"
                    + "LedSdk 的界面依赖 WebView2，无法继续安装。是否打开官方下载页面？",
                    "缺少 WebView2",
                    MessageBoxButtons.YesNo,
                    MessageBoxIcon.Warning);
                if (open == DialogResult.Yes)
                {
                    try
                    {
                        Process.Start(new ProcessStartInfo(WebView2DownloadUrl) { UseShellExecute = true });
                    }
                    catch { }
                }
                return 1;
            }
            if (silent)
            {
                InstallOptions options = new InstallOptions();
                options.TargetDir = ArgValue(args, "/dir");
                options.DesktopShortcut = !HasArg(args, "/nodesktop");
                options.StartMenuShortcut = !HasArg(args, "/nostartmenu");
                options.LaunchAfterInstall = HasArg(args, "/launch");
                string log = Path.Combine(Path.GetTempPath(), "LedSdkSetup.log");
                try { File.WriteAllText(log, ""); } catch { }
                try
                {
                    Installer.Install(options, delegate(string message) { Log(log, message); });
                    Log(log, "安装成功。");
                    return 0;
                }
                catch (Exception ex)
                {
                    Log(log, "安装失败：" + ex);
                    return 1;
                }
            }

            using (SetupForm form = new SetupForm())
            {
                Application.Run(form);
                return form.ExitCode;
            }
        }

        private static void Log(string path, string message)
        {
            try
            {
                File.AppendAllText(path, DateTime.Now.ToString("yyyy-MM-dd HH:mm:ss") + " " + message + Environment.NewLine, Encoding.UTF8);
            }
            catch { }
        }

        private static bool HasArg(string[] args, string name)
        {
            foreach (string item in args)
            {
                if (string.Equals(item, name, StringComparison.OrdinalIgnoreCase)) return true;
            }
            return false;
        }

        private static string ArgValue(string[] args, string name)
        {
            for (int i = 0; i < args.Length - 1; i++)
            {
                if (string.Equals(args[i], name, StringComparison.OrdinalIgnoreCase)) return args[i + 1];
            }
            return null;
        }

        private static int VerifyPayload(string dest)
        {
            if (string.IsNullOrEmpty(dest))
            {
                dest = Path.Combine(Path.GetTempPath(), "LedSdkPayloadVerify-" + Guid.NewGuid().ToString("N"));
            }
            try
            {
                Installer.ExtractPayload(dest);
                string exe = Path.Combine(dest, "ledsdk-tauri.exe");
                string config = Path.Combine(dest, "config", "appsettings.json");
                string setup = Path.Combine(dest, "setup.ps1");
                if (!File.Exists(exe) || !File.Exists(config) || !File.Exists(setup))
                {
                    throw new InvalidOperationException("负载缺少 ledsdk-tauri.exe / config / setup.ps1。");
                }
                int count = 0;
                long bytes = 0;
                foreach (string file in Directory.GetFiles(dest, "*", SearchOption.AllDirectories))
                {
                    count++;
                    bytes += new FileInfo(file).Length;
                }
                string report = "PASSED: " + count + " files, " + bytes + " bytes, dir=" + dest;
                File.WriteAllText(Path.Combine(dest, "verify-report.txt"), report, Encoding.UTF8);
                return 0;
            }
            catch (Exception ex)
            {
                try
                {
                    Directory.CreateDirectory(dest);
                    File.WriteAllText(Path.Combine(dest, "verify-report.txt"), "FAILED: " + ex, Encoding.UTF8);
                }
                catch { }
                return 1;
            }
        }

        private static int RunUninstall()
        {
            string target = Installer.GetRegisteredInstallDir();
            if (string.IsNullOrEmpty(target)) target = Installer.DefaultInstallDir;

            string uninstaller = Path.Combine(target, "uninstall.exe");
            if (File.Exists(uninstaller))
            {
                try
                {
                    Process.Start(new ProcessStartInfo(uninstaller) { UseShellExecute = true });
                    return 0;
                }
                catch (Exception ex)
                {
                    MessageBox.Show(ex.Message, "卸载失败", MessageBoxButtons.OK, MessageBoxIcon.Error);
                    return 1;
                }
            }

            string script = Path.Combine(target, "uninstall.ps1");
            if (!File.Exists(script))
            {
                MessageBox.Show("未找到卸载脚本，可能尚未安装。", "LedSdk", MessageBoxButtons.OK, MessageBoxIcon.Warning);
                return 1;
            }
            try
            {
                Installer.RunPowerShellInteractive(script, " -Uninstall");
                return 0;
            }
            catch (Exception ex)
            {
                MessageBox.Show(ex.Message, "卸载失败", MessageBoxButtons.OK, MessageBoxIcon.Error);
                return 1;
            }
        }

        private static string Preflight()
        {
            if (!Environment.Is64BitOperatingSystem)
            {
                return "LedSdk 仅支持 64 位 Windows 10 / Windows 11。";
            }

            // 未声明 Windows 10 兼容清单的 .NET 程序，Environment.OSVersion 可能返回 6.2；
            // RtlGetVersion 才能拿到真实版本。
            OSVERSIONINFOEX info = new OSVERSIONINFOEX();
            info.dwOSVersionInfoSize = Marshal.SizeOf(typeof(OSVERSIONINFOEX));
            int major = 0;
            int minor = 0;
            int build = 0;
            try
            {
                if (RtlGetVersion(ref info) == 0)
                {
                    major = info.dwMajorVersion;
                    minor = info.dwMinorVersion;
                    build = info.dwBuildNumber;
                }
            }
            catch { }
            if (major == 0)
            {
                Version version = Environment.OSVersion.Version;
                major = version.Major;
                minor = version.Minor;
                build = version.Build;
            }
            if (major < 10 || (major == 10 && build < 17763))
            {
                return "LedSdk 需要 Windows 10 1809 或更高版本。\r\n当前系统："
                    + major + "." + minor + "." + build;
            }
            return null;
        }

        private static bool HasWebView2()
        {
            string[] machineKeys =
            {
                @"SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}",
                @"SOFTWARE\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}"
            };
            foreach (string keyPath in machineKeys)
            {
                using (RegistryKey key = Registry.LocalMachine.OpenSubKey(keyPath))
                {
                    if (key != null && !string.IsNullOrEmpty(key.GetValue("pv") as string)) return true;
                }
            }
            using (RegistryKey key = Registry.CurrentUser.OpenSubKey(
                @"Software\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}"))
            {
                if (key != null && !string.IsNullOrEmpty(key.GetValue("pv") as string)) return true;
            }

            string[] roots =
            {
                Environment.GetFolderPath(Environment.SpecialFolder.ProgramFilesX86),
                Environment.GetFolderPath(Environment.SpecialFolder.ProgramFiles)
            };
            foreach (string root in roots)
            {
                if (string.IsNullOrEmpty(root)) continue;
                string app = Path.Combine(root, "Microsoft", "EdgeWebView", "Application");
                if (!Directory.Exists(app)) continue;
                try
                {
                    foreach (string dir in Directory.GetDirectories(app))
                    {
                        if (File.Exists(Path.Combine(dir, "msedgewebview2.exe"))) return true;
                    }
                }
                catch { }
            }
            return false;
        }
    }

    internal static class Installer
    {
        internal static string DefaultInstallDir
        {
            get
            {
                return Path.Combine(
                    Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),
                    "Programs",
                    "LedSdk");
            }
        }

        /// <summary>读取当前用户卸载项中记录的安装目录，用于升级时沿用原路径。</summary>
        internal static string GetRegisteredInstallDir()
        {
            try
            {
                using (RegistryKey key = Registry.CurrentUser.OpenSubKey(
                    @"Software\Microsoft\Windows\CurrentVersion\Uninstall\LedSdk"))
                {
                    if (key == null) return null;
                    string value = key.GetValue("InstallLocation") as string;
                    if (string.IsNullOrWhiteSpace(value)) return null;
                    value = value.Trim().Trim('"');
                    return Path.IsPathRooted(value) ? Path.GetFullPath(value) : null;
                }
            }
            catch
            {
                return null;
            }
        }

        /// <summary>校验并规范化用户选择的安装目录，并确认目录可写。</summary>
        internal static string ResolveTargetDir(string value)
        {
            string input = string.IsNullOrWhiteSpace(value)
                ? DefaultInstallDir
                : value.Replace("\"", "").Trim();
            // 命令行传参时，路径末尾的反斜杠会转义结束引号；兼容还原盘符根路径。
            if (input.Length == 2 && input[1] == ':') input += Path.DirectorySeparatorChar;
            if (!Path.IsPathRooted(input)) throw new ArgumentException("安装路径必须是绝对路径。");

            string target = Path.GetFullPath(input).TrimEnd(
                Path.DirectorySeparatorChar,
                Path.AltDirectorySeparatorChar);
            if (target.Length <= 3) throw new ArgumentException("不能直接安装到磁盘根目录，请选择子目录。");
            if (File.Exists(target)) throw new ArgumentException("安装路径已被文件占用：" + target);

            string windows = Path.GetFullPath(
                Environment.GetFolderPath(Environment.SpecialFolder.Windows)).TrimEnd(
                    Path.DirectorySeparatorChar,
                    Path.AltDirectorySeparatorChar);
            if (IsSameOrChild(target, windows))
            {
                throw new ArgumentException("不能安装到 Windows 系统目录，请选择其他目录。");
            }

            string probe = Path.Combine(target, ".ledsdk-write-test-" + Guid.NewGuid().ToString("N"));
            try
            {
                Directory.CreateDirectory(target);
                File.WriteAllText(probe, "test", Encoding.UTF8);
            }
            catch (Exception ex)
            {
                throw new InvalidOperationException(
                    "安装目录不可写，请选择有写入权限的目录：\r\n" + target + "\r\n\r\n" + ex.Message);
            }
            finally
            {
                try
                {
                    if (File.Exists(probe)) File.Delete(probe);
                }
                catch { }
            }
            return target;
        }

        private static bool IsSameOrChild(string path, string parent)
        {
            if (string.Equals(path, parent, StringComparison.OrdinalIgnoreCase)) return true;
            return path.StartsWith(parent + Path.DirectorySeparatorChar, StringComparison.OrdinalIgnoreCase);
        }

        private static string PowerShellPath
        {
            get
            {
                return Path.Combine(
                    Environment.GetFolderPath(Environment.SpecialFolder.Windows),
                    "System32",
                    "WindowsPowerShell",
                    "v1.0",
                    "powershell.exe");
            }
        }

        internal static void Install(InstallOptions options, Action<string> progress)
        {
            string temp = Path.Combine(Path.GetTempPath(), "LedSdkSetup-" + Guid.NewGuid().ToString("N"));
            string target = ResolveTargetDir(options.TargetDir);
            try
            {
                progress("正在解压安装文件…");
                ExtractPayload(temp);
                progress("正在检查运行库…");
                if (!EnsureVcRuntime(temp, progress))
                {
                    throw new InvalidOperationException(
                        "未安装 VC++ 2013 x64 运行库。请先运行安装包中的 vcredist_x64.exe，然后重试。");
                }
                List<Process> running = FindAppProcesses(target);
                if (running.Count > 0)
                {
                    progress("正在停止 LedSdk…");
                    StopAppProcesses(running);
                }
                string setup = Path.Combine(temp, "setup.ps1");
                if (!File.Exists(setup)) throw new FileNotFoundException("安装包缺少 setup.ps1", setup);

                string extra = " -Quiet";
                extra += " -TargetDir \"" + target + "\"";
                if (!options.DesktopShortcut) extra += " -NoDesktopShortcut";
                if (!options.StartMenuShortcut) extra += " -NoStartMenuShortcut";

                progress("正在安装到 " + target + " …");
                RunPowerShell(setup, extra);

                Directory.CreateDirectory(target);
                string exe = Path.Combine(target, "ledsdk-tauri.exe");
                if (!File.Exists(exe))
                {
                    throw new InvalidOperationException("安装脚本未生成程序文件：" + exe);
                }
                string uninstaller = Path.Combine(target, "uninstall.exe");
                bool hasUninstaller = File.Exists(uninstaller);
                if (!hasUninstaller)
                {
                    // 兼容不含独立卸载程序的旧负载。
                    File.Copy(setup, Path.Combine(target, "uninstall.ps1"), true);
                }
                WriteUninstallEntry(target, hasUninstaller);

                progress("安装完成。");
                if (options.LaunchAfterInstall)
                {
                    try
                    {
                        Process.Start(exe);
                    }
                    catch { }
                }
            }
            finally
            {
                TryDeleteDirectory(temp);
            }
        }

        internal static List<Process> FindAppProcesses(string target)
        {
            string expected = Path.GetFullPath(Path.Combine(target, "ledsdk-tauri.exe"));
            List<Process> result = new List<Process>();
            foreach (Process process in Process.GetProcessesByName("ledsdk-tauri"))
            {
                bool match = true;
                try
                {
                    match = string.Equals(
                        Path.GetFullPath(process.MainModule.FileName),
                        expected,
                        StringComparison.OrdinalIgnoreCase);
                }
                catch
                {
                    // 读不到路径时按进程名处理；ledsdk-tauri 名称唯一。
                }
                if (match) result.Add(process);
            }
            return result;
        }

        internal static void StopAppProcesses(List<Process> processes)
        {
            foreach (Process process in processes)
            {
                try
                {
                    process.Kill();
                    process.WaitForExit(3000);
                }
                catch { }
            }
            Thread.Sleep(500);
        }

        private static bool EnsureVcRuntime(string payloadDir, Action<string> progress)
        {
            if (HasVcRuntime()) return true;
            string redist = Path.Combine(payloadDir, "vcredist_x64.exe");
            if (!File.Exists(redist)) return false;

            progress("正在安装 VC++ 2013 x64 运行库…");
            try
            {
                ProcessStartInfo info = new ProcessStartInfo(redist, "/install /quiet /norestart");
                info.UseShellExecute = true;
                info.Verb = "runas";
                using (Process process = Process.Start(info))
                {
                    process.WaitForExit();
                    int code = process.ExitCode;
                    if (code != 0 && code != 1638 && code != 3010) return false;
                }
            }
            catch
            {
                return false;
            }
            Thread.Sleep(1000);
            return HasVcRuntime();
        }

        private static bool HasVcRuntime()
        {
            string system = Environment.GetFolderPath(Environment.SpecialFolder.System);
            return File.Exists(Path.Combine(system, "msvcp120.dll"))
                && File.Exists(Path.Combine(system, "msvcr120.dll"));
        }

        internal static void ExtractPayload(string dest)
        {
            Directory.CreateDirectory(dest);
            string root = Path.GetFullPath(dest);
            if (!root.EndsWith(Path.DirectorySeparatorChar.ToString())) root += Path.DirectorySeparatorChar;

            using (Stream stream = Assembly.GetExecutingAssembly().GetManifestResourceStream(Program.PayloadResource))
            {
                if (stream == null) throw new InvalidOperationException("安装包缺少内嵌负载。");
                using (ZipArchive archive = new ZipArchive(stream, ZipArchiveMode.Read))
                {
                    foreach (ZipArchiveEntry entry in archive.Entries)
                    {
                        string full = Path.GetFullPath(Path.Combine(dest, entry.FullName));
                        if (!full.StartsWith(root, StringComparison.OrdinalIgnoreCase))
                        {
                            throw new InvalidOperationException("非法的压缩包路径：" + entry.FullName);
                        }
                        if (entry.FullName.EndsWith("/") || entry.FullName.EndsWith("\\"))
                        {
                            Directory.CreateDirectory(full);
                            continue;
                        }
                        string dir = Path.GetDirectoryName(full);
                        if (!string.IsNullOrEmpty(dir)) Directory.CreateDirectory(dir);
                        entry.ExtractToFile(full, true);
                    }
                }
            }
        }

        internal static void RunPowerShell(string script, string extraArgs)
        {
            ProcessStartInfo info = new ProcessStartInfo();
            info.FileName = PowerShellPath;
            info.Arguments = "-NoProfile -ExecutionPolicy Bypass -File \"" + script + "\"" + extraArgs;
            info.UseShellExecute = false;
            info.CreateNoWindow = true;
            info.RedirectStandardOutput = true;
            info.RedirectStandardError = true;
            info.WorkingDirectory = Path.GetDirectoryName(script);

            using (Process process = Process.Start(info))
            {
                string stdout = process.StandardOutput.ReadToEnd();
                string stderr = process.StandardError.ReadToEnd();
                process.WaitForExit();
                if (process.ExitCode != 0)
                {
                    throw new InvalidOperationException(
                        "安装脚本失败（退出码 " + process.ExitCode + "）：\r\n" + stdout + "\r\n" + stderr);
                }
            }
        }

        internal static void RunPowerShellInteractive(string script, string extraArgs)
        {
            ProcessStartInfo info = new ProcessStartInfo();
            info.FileName = PowerShellPath;
            info.Arguments = "-NoProfile -ExecutionPolicy Bypass -File \"" + script + "\"" + extraArgs;
            info.UseShellExecute = true;
            info.WorkingDirectory = Path.GetDirectoryName(script);
            using (Process process = Process.Start(info))
            {
                process.WaitForExit();
            }
        }

        private static void WriteUninstallEntry(string target, bool hasUninstaller)
        {
            using (RegistryKey key = Registry.CurrentUser.CreateSubKey(
                @"Software\Microsoft\Windows\CurrentVersion\Uninstall\LedSdk"))
            {
                key.SetValue("DisplayName", "LedSdk 屏幕节目上传服务");
                key.SetValue("DisplayVersion", Program.Version);
                key.SetValue("Publisher", "LedSdk");
                key.SetValue("InstallLocation", target);
                if (hasUninstaller)
                {
                    string uninstaller = Path.Combine(target, "uninstall.exe");
                    key.SetValue("DisplayIcon", uninstaller + ",0");
                    key.SetValue("UninstallString", "\"" + uninstaller + "\"");
                    key.SetValue("QuietUninstallString", "\"" + uninstaller + "\" --quiet");
                }
                else
                {
                    key.SetValue("DisplayIcon", Path.Combine(target, "ledsdk-tauri.exe") + ",0");
                    key.SetValue(
                        "UninstallString",
                        "\"" + PowerShellPath + "\" -NoProfile -ExecutionPolicy Bypass -File \""
                        + Path.Combine(target, "uninstall.ps1") + "\" -Uninstall -TargetDir \""
                        + target + "\"");
                }
                key.SetValue("NoModify", 1, RegistryValueKind.DWord);
                key.SetValue("NoRepair", 1, RegistryValueKind.DWord);
            }
        }

        private static void TryDeleteDirectory(string dir)
        {
            try
            {
                if (Directory.Exists(dir)) Directory.Delete(dir, true);
            }
            catch { }
        }
    }

    internal sealed class SetupForm : Form
    {
        private readonly TextBox pathBox;
        private readonly CheckBox desktop;
        private readonly CheckBox startMenu;
        private readonly CheckBox launch;
        private readonly Button install;
        private readonly Button cancel;
        private readonly Button browse;
        private readonly ProgressBar progress;
        private readonly Label status;
        private bool busy;

        internal int ExitCode = 0;

        internal SetupForm()
        {
            Text = "LedSdk 安装程序";
            AutoScaleMode = AutoScaleMode.Dpi;
            AutoScaleDimensions = new SizeF(96F, 96F);
            ClientSize = new Size(720, 510);
            FormBorderStyle = FormBorderStyle.FixedDialog;
            MaximizeBox = false;
            MinimizeBox = false;
            StartPosition = FormStartPosition.CenterScreen;
            Font = new Font("Microsoft YaHei UI", 9F);
            try { Icon = Icon.ExtractAssociatedIcon(Application.ExecutablePath); } catch { }

            TableLayoutPanel root = new TableLayoutPanel();
            root.Dock = DockStyle.Fill;
            root.Padding = new Padding(24, 20, 24, 18);
            root.ColumnCount = 1;
            root.RowCount = 6;
            root.ColumnStyles.Add(new ColumnStyle(SizeType.Percent, 100F));
            root.RowStyles.Add(new RowStyle(SizeType.AutoSize));
            root.RowStyles.Add(new RowStyle(SizeType.AutoSize));
            root.RowStyles.Add(new RowStyle(SizeType.AutoSize));
            root.RowStyles.Add(new RowStyle(SizeType.Percent, 100F));
            root.RowStyles.Add(new RowStyle(SizeType.Absolute, 78F));
            root.RowStyles.Add(new RowStyle(SizeType.Absolute, 44F));
            Controls.Add(root);

            TableLayoutPanel header = new TableLayoutPanel();
            header.AutoSize = true;
            header.AutoSizeMode = AutoSizeMode.GrowAndShrink;
            header.Dock = DockStyle.Fill;
            header.ColumnCount = 1;
            header.RowCount = 2;
            header.Margin = new Padding(0, 0, 0, 10);
            header.ColumnStyles.Add(new ColumnStyle(SizeType.Percent, 100F));
            header.RowStyles.Add(new RowStyle(SizeType.AutoSize));
            header.RowStyles.Add(new RowStyle(SizeType.AutoSize));
            root.Controls.Add(header, 0, 0);

            Label title = new Label();
            title.Text = "LedSdk 屏幕节目上传服务";
            title.Font = new Font(Font.FontFamily, 16F, FontStyle.Bold);
            title.AutoSize = true;
            title.Margin = new Padding(0, 0, 0, 4);
            header.Controls.Add(title, 0, 0);

            Label version = new Label();
            version.Text = "版本 " + Program.Version + " · 安装向导";
            version.AutoSize = true;
            version.ForeColor = Color.Gray;
            version.Margin = new Padding(1, 0, 0, 0);
            header.Controls.Add(version, 0, 1);

            GroupBox pathGroup = new GroupBox();
            pathGroup.Text = "安装位置";
            pathGroup.AutoSize = true;
            pathGroup.AutoSizeMode = AutoSizeMode.GrowAndShrink;
            pathGroup.Dock = DockStyle.Top;
            pathGroup.Padding = new Padding(12, 8, 12, 8);
            pathGroup.Margin = new Padding(0, 0, 0, 10);
            root.Controls.Add(pathGroup, 0, 1);

            TableLayoutPanel pathLayout = new TableLayoutPanel();
            pathLayout.AutoSize = true;
            pathLayout.AutoSizeMode = AutoSizeMode.GrowAndShrink;
            pathLayout.Dock = DockStyle.Top;
            pathLayout.ColumnCount = 3;
            pathLayout.RowCount = 2;
            pathLayout.ColumnStyles.Add(new ColumnStyle(SizeType.Absolute, 112F));
            pathLayout.ColumnStyles.Add(new ColumnStyle(SizeType.Percent, 100F));
            pathLayout.ColumnStyles.Add(new ColumnStyle(SizeType.Absolute, 96F));
            pathLayout.RowStyles.Add(new RowStyle(SizeType.Absolute, 34F));
            pathLayout.RowStyles.Add(new RowStyle(SizeType.AutoSize));
            pathGroup.Controls.Add(pathLayout);

            Label pathLabel = new Label();
            pathLabel.Text = "安装到：";
            pathLabel.AutoSize = true;
            pathLabel.Dock = DockStyle.Fill;
            pathLabel.TextAlign = ContentAlignment.MiddleLeft;
            pathLabel.Margin = new Padding(0, 0, 4, 0);
            pathLayout.Controls.Add(pathLabel, 0, 0);

            pathBox = new TextBox();
            string initialTarget = Installer.GetRegisteredInstallDir();
            if (string.IsNullOrEmpty(initialTarget)) initialTarget = Installer.DefaultInstallDir;
            pathBox.Text = initialTarget;
            pathBox.Dock = DockStyle.Fill;
            pathBox.Margin = new Padding(0, 4, 8, 4);
            pathLayout.Controls.Add(pathBox, 1, 0);

            browse = new Button();
            browse.Text = "浏览…";
            browse.Dock = DockStyle.Fill;
            browse.Margin = new Padding(0, 3, 0, 3);
            browse.Click += OnBrowse;
            pathLayout.Controls.Add(browse, 2, 0);

            Label pathHint = new Label();
            pathHint.Text = "可选择任意具有写入权限的本地目录；升级时会优先沿用原安装位置。";
            pathHint.AutoSize = true;
            pathHint.ForeColor = Color.Gray;
            pathHint.TextAlign = ContentAlignment.MiddleLeft;
            pathHint.Margin = new Padding(0, 4, 0, 0);
            pathLayout.Controls.Add(pathHint, 0, 1);
            pathLayout.SetColumnSpan(pathHint, 3);

            GroupBox optionsGroup = new GroupBox();
            optionsGroup.Text = "安装选项";
            optionsGroup.AutoSize = true;
            optionsGroup.AutoSizeMode = AutoSizeMode.GrowAndShrink;
            optionsGroup.Dock = DockStyle.Top;
            optionsGroup.Padding = new Padding(12, 6, 12, 6);
            optionsGroup.Margin = new Padding(0, 0, 0, 10);
            root.Controls.Add(optionsGroup, 0, 2);

            TableLayoutPanel optionLayout = new TableLayoutPanel();
            optionLayout.AutoSize = true;
            optionLayout.AutoSizeMode = AutoSizeMode.GrowAndShrink;
            optionLayout.Dock = DockStyle.Top;
            optionLayout.ColumnCount = 1;
            optionLayout.RowCount = 3;
            optionLayout.ColumnStyles.Add(new ColumnStyle(SizeType.Percent, 100F));
            optionLayout.RowStyles.Add(new RowStyle(SizeType.AutoSize));
            optionLayout.RowStyles.Add(new RowStyle(SizeType.AutoSize));
            optionLayout.RowStyles.Add(new RowStyle(SizeType.AutoSize));
            optionsGroup.Controls.Add(optionLayout);

            desktop = NewOption("创建桌面图标", true);
            startMenu = NewOption("创建开始菜单图标", true);
            launch = NewOption("安装完成后启动 LedSdk", true);
            optionLayout.Controls.Add(desktop, 0, 0);
            optionLayout.Controls.Add(startMenu, 0, 1);
            optionLayout.Controls.Add(launch, 0, 2);

            TableLayoutPanel statusLayout = new TableLayoutPanel();
            statusLayout.Dock = DockStyle.Fill;
            statusLayout.ColumnCount = 1;
            statusLayout.RowCount = 2;
            statusLayout.ColumnStyles.Add(new ColumnStyle(SizeType.Percent, 100F));
            statusLayout.RowStyles.Add(new RowStyle(SizeType.Absolute, 18F));
            statusLayout.RowStyles.Add(new RowStyle(SizeType.Percent, 100F));
            statusLayout.Margin = new Padding(0, 0, 0, 8);
            root.Controls.Add(statusLayout, 0, 4);

            progress = new ProgressBar();
            progress.Style = ProgressBarStyle.Marquee;
            progress.MarqueeAnimationSpeed = 30;
            progress.Dock = DockStyle.Fill;
            progress.Margin = new Padding(0, 0, 0, 6);
            progress.Visible = false;
            statusLayout.Controls.Add(progress, 0, 0);

            status = new Label();
            status.Text = "确认安装位置和选项后，点击「安装」开始。";
            status.AutoSize = false;
            status.Dock = DockStyle.Fill;
            status.TextAlign = ContentAlignment.TopLeft;
            status.Margin = new Padding(1, 0, 0, 0);
            statusLayout.Controls.Add(status, 0, 1);

            FlowLayoutPanel buttons = new FlowLayoutPanel();
            buttons.Dock = DockStyle.Fill;
            buttons.FlowDirection = FlowDirection.RightToLeft;
            buttons.WrapContents = false;
            buttons.Margin = new Padding(0);
            root.Controls.Add(buttons, 0, 5);

            install = NewDialogButton("安装");
            install.Click += OnInstall;
            buttons.Controls.Add(install);

            cancel = NewDialogButton("取消");
            cancel.Click += delegate { Close(); };
            buttons.Controls.Add(cancel);

            AcceptButton = install;
            CancelButton = cancel;
            ActiveControl = install;
            pathBox.SelectionStart = 0;
            pathBox.SelectionLength = 0;
            FormClosing += OnFormClosing;
        }

        private CheckBox NewOption(string text, bool isChecked)
        {
            CheckBox box = new CheckBox();
            box.Text = text;
            box.Checked = isChecked;
            box.AutoSize = true;
            box.Margin = new Padding(2, 3, 0, 3);
            return box;
        }

        private Button NewDialogButton(string text)
        {
            Button button = new Button();
            button.Text = text;
            button.Size = new Size(96, 32);
            button.Margin = new Padding(8, 0, 0, 0);
            return button;
        }

        private void OnBrowse(object sender, EventArgs e)
        {
            using (FolderBrowserDialog dialog = new FolderBrowserDialog())
            {
                dialog.Description = "选择 LedSdk 安装目录";
                dialog.ShowNewFolderButton = true;
                string current = pathBox.Text.Trim();
                if (Directory.Exists(current))
                {
                    dialog.SelectedPath = current;
                }
                else
                {
                    try
                    {
                        string parent = Path.GetDirectoryName(current);
                        if (!string.IsNullOrEmpty(parent) && Directory.Exists(parent))
                        {
                            dialog.SelectedPath = parent;
                        }
                    }
                    catch { }
                }
                if (dialog.ShowDialog(this) == DialogResult.OK)
                {
                    pathBox.Text = dialog.SelectedPath;
                }
            }
        }

        private void OnFormClosing(object sender, FormClosingEventArgs e)
        {
            if (busy) e.Cancel = true;
        }

        private void OnInstall(object sender, EventArgs e)
        {
            if (busy) return;

            string target;
            try
            {
                target = Installer.ResolveTargetDir(pathBox.Text);
                pathBox.Text = target;
            }
            catch (Exception ex)
            {
                status.Text = "安装位置无效，请重新选择。";
                MessageBox.Show(this, ex.Message, "安装位置无效", MessageBoxButtons.OK, MessageBoxIcon.Warning);
                pathBox.Focus();
                pathBox.SelectAll();
                return;
            }

            List<Process> running = Installer.FindAppProcesses(target);
            if (running.Count > 0)
            {
                DialogResult answer = MessageBox.Show(
                    this,
                    "LedSdk 正在运行，是否强制停止？",
                    "LedSdk 安装",
                    MessageBoxButtons.YesNo,
                    MessageBoxIcon.Warning);
                if (answer != DialogResult.Yes)
                {
                    status.Text = "已取消安装。";
                    return;
                }
                Installer.StopAppProcesses(running);
            }

            busy = true;
            install.Enabled = false;
            cancel.Enabled = false;
            desktop.Enabled = false;
            startMenu.Enabled = false;
            launch.Enabled = false;
            browse.Enabled = false;
            pathBox.Enabled = false;
            progress.Visible = true;
            status.Text = "正在准备…";

            InstallOptions options = new InstallOptions();
            options.TargetDir = target;
            options.DesktopShortcut = desktop.Checked;
            options.StartMenuShortcut = startMenu.Checked;
            options.LaunchAfterInstall = launch.Checked;

            Thread thread = new Thread(delegate()
            {
                try
                {
                    Installer.Install(options, SetStatus);
                    BeginInvoke(new MethodInvoker(delegate()
                    {
                        progress.Visible = false;
                        status.Text = "安装完成。";
                        MessageBox.Show(
                            this,
                            "LedSdk 已安装到：\r\n" + target,
                            "安装完成",
                            MessageBoxButtons.OK,
                            MessageBoxIcon.Information);
                        ExitCode = 0;
                        busy = false;
                        Close();
                    }));
                }
                catch (Exception ex)
                {
                    BeginInvoke(new MethodInvoker(delegate()
                    {
                        progress.Visible = false;
                        busy = false;
                        install.Enabled = true;
                        cancel.Enabled = true;
                        desktop.Enabled = true;
                        startMenu.Enabled = true;
                        launch.Enabled = true;
                        browse.Enabled = true;
                        pathBox.Enabled = true;
                        status.Text = "安装失败。";
                        MessageBox.Show(this, ex.Message, "安装失败", MessageBoxButtons.OK, MessageBoxIcon.Error);
                    }));
                }
            });
            thread.IsBackground = true;
            thread.Start();
        }

        private void SetStatus(string text)
        {
            if (IsDisposed) return;
            if (InvokeRequired)
            {
                BeginInvoke(new MethodInvoker(delegate { status.Text = text; }));
            }
            else
            {
                status.Text = text;
            }
        }
    }
}
