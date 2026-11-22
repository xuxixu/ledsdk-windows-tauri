// LedSdk 单文件安装程序：内嵌 dist 发行负载，复用 setup.ps1 完成安装。
// 目标框架为 Windows 自带的 .NET Framework 4.8，构建时调用系统 csc.exe，不引入第三方依赖。
using System;
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
[assembly: AssemblyVersion("1.0.0.0")]
[assembly: AssemblyFileVersion("1.0.0.0")]

namespace LedSdkSetup
{
    internal sealed class InstallOptions
    {
        public bool DesktopShortcut = true;
        public bool StartMenuShortcut = true;
        public bool LaunchAfterInstall = true;
    }

    internal static class Program
    {
        internal const string Version = "1.0.0";
        internal const string PayloadResource = "LedSdkPayload.zip";

        [DllImport("user32.dll")]
        private static extern bool SetProcessDPIAware();

        [STAThread]
        private static int Main(string[] args)
        {
            try { SetProcessDPIAware(); } catch { }
            Application.EnableVisualStyles();
            Application.SetCompatibleTextRenderingDefault(false);

            if (HasArg(args, "/verify"))
            {
                return VerifyPayload(ArgValue(args, "/verify"));
            }
            if (HasArg(args, "/uninstall"))
            {
                return RunUninstall();
            }
            if (HasArg(args, "/silent"))
            {
                InstallOptions options = new InstallOptions();
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
            string script = Path.Combine(Installer.InstallDir, "uninstall.ps1");
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
    }

    internal static class Installer
    {
        internal static string InstallDir
        {
            get
            {
                return Path.Combine(
                    Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),
                    "Programs",
                    "LedSdk");
            }
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
            try
            {
                progress("正在解压安装文件…");
                ExtractPayload(temp);
                string setup = Path.Combine(temp, "setup.ps1");
                if (!File.Exists(setup)) throw new FileNotFoundException("安装包缺少 setup.ps1", setup);

                string extra = " -Quiet";
                if (!options.DesktopShortcut) extra += " -NoDesktopShortcut";
                if (!options.StartMenuShortcut) extra += " -NoStartMenuShortcut";

                progress("正在安装到 " + InstallDir + " …");
                RunPowerShell(setup, extra);

                Directory.CreateDirectory(InstallDir);
                File.Copy(setup, Path.Combine(InstallDir, "uninstall.ps1"), true);
                WriteUninstallEntry();

                progress("安装完成。");
                if (options.LaunchAfterInstall)
                {
                    try
                    {
                        Process.Start(Path.Combine(InstallDir, "ledsdk-tauri.exe"));
                    }
                    catch { }
                }
            }
            finally
            {
                TryDeleteDirectory(temp);
            }
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

        private static void WriteUninstallEntry()
        {
            using (RegistryKey key = Registry.CurrentUser.CreateSubKey(
                @"Software\Microsoft\Windows\CurrentVersion\Uninstall\LedSdk"))
            {
                key.SetValue("DisplayName", "LedSdk 屏幕节目上传服务");
                key.SetValue("DisplayVersion", Program.Version);
                key.SetValue("Publisher", "LedSdk");
                key.SetValue("InstallLocation", InstallDir);
                key.SetValue("DisplayIcon", Path.Combine(InstallDir, "ledsdk-tauri.exe") + ",0");
                key.SetValue(
                    "UninstallString",
                    "\"" + PowerShellPath + "\" -NoProfile -ExecutionPolicy Bypass -File \""
                    + Path.Combine(InstallDir, "uninstall.ps1") + "\" -Uninstall");
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
        private readonly CheckBox desktop;
        private readonly CheckBox startMenu;
        private readonly CheckBox launch;
        private readonly Button install;
        private readonly Button cancel;
        private readonly ProgressBar progress;
        private readonly Label status;
        private bool busy;

        internal int ExitCode = 0;

        internal SetupForm()
        {
            Text = "LedSdk 安装程序";
            ClientSize = new Size(560, 350);
            FormBorderStyle = FormBorderStyle.FixedDialog;
            MaximizeBox = false;
            MinimizeBox = false;
            StartPosition = FormStartPosition.CenterScreen;
            Font = new Font("Microsoft YaHei UI", 9F);
            try { Icon = Icon.ExtractAssociatedIcon(Application.ExecutablePath); } catch { }

            Label title = new Label();
            title.Text = "LedSdk 屏幕节目上传服务";
            title.Font = new Font(Font.FontFamily, 15F, FontStyle.Bold);
            title.AutoSize = true;
            title.Location = new Point(24, 22);
            Controls.Add(title);

            Label version = new Label();
            version.Text = "版本 " + Program.Version;
            version.AutoSize = true;
            version.ForeColor = Color.Gray;
            version.Location = new Point(26, 58);
            Controls.Add(version);

            Label path = new Label();
            path.Text = "安装位置：" + Installer.InstallDir;
            path.AutoEllipsis = true;
            path.AutoSize = false;
            path.Location = new Point(26, 92);
            path.Size = new Size(508, 20);
            Controls.Add(path);

            desktop = new CheckBox();
            desktop.Text = "创建桌面图标";
            desktop.Checked = true;
            desktop.AutoSize = true;
            desktop.Location = new Point(28, 130);
            Controls.Add(desktop);

            startMenu = new CheckBox();
            startMenu.Text = "创建开始菜单图标";
            startMenu.Checked = true;
            startMenu.AutoSize = true;
            startMenu.Location = new Point(28, 158);
            Controls.Add(startMenu);

            launch = new CheckBox();
            launch.Text = "安装完成后启动 LedSdk";
            launch.Checked = true;
            launch.AutoSize = true;
            launch.Location = new Point(28, 186);
            Controls.Add(launch);

            progress = new ProgressBar();
            progress.Style = ProgressBarStyle.Marquee;
            progress.MarqueeAnimationSpeed = 30;
            progress.Location = new Point(28, 222);
            progress.Size = new Size(504, 16);
            progress.Visible = false;
            Controls.Add(progress);

            status = new Label();
            status.Text = "点击「安装」开始。";
            status.AutoSize = false;
            status.Location = new Point(28, 246);
            status.Size = new Size(504, 40);
            Controls.Add(status);

            install = new Button();
            install.Text = "安装";
            install.Location = new Point(356, 300);
            install.Size = new Size(80, 30);
            install.Click += OnInstall;
            Controls.Add(install);

            cancel = new Button();
            cancel.Text = "取消";
            cancel.Location = new Point(452, 300);
            cancel.Size = new Size(80, 30);
            cancel.Click += delegate { Close(); };
            Controls.Add(cancel);

            AcceptButton = install;
            CancelButton = cancel;
            FormClosing += OnFormClosing;
        }

        private void OnFormClosing(object sender, FormClosingEventArgs e)
        {
            if (busy) e.Cancel = true;
        }

        private void OnInstall(object sender, EventArgs e)
        {
            if (busy) return;
            busy = true;
            install.Enabled = false;
            cancel.Enabled = false;
            desktop.Enabled = false;
            startMenu.Enabled = false;
            launch.Enabled = false;
            progress.Visible = true;
            status.Text = "正在准备…";

            InstallOptions options = new InstallOptions();
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
                            "LedSdk 已安装到：\r\n" + Installer.InstallDir,
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
