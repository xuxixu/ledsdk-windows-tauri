// LedSdk 卸载程序：检查/停止运行中的实例，按勾选项删除快捷方式、自启与用户数据。
// 目标框架为 Windows 自带的 .NET Framework 4.8，构建时调用系统 csc.exe，不引入第三方依赖。
using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Drawing;
using System.IO;
using System.Reflection;
using System.Runtime.InteropServices;
using System.Text;
using System.Threading;
using System.Windows.Forms;
using Microsoft.Win32;

[assembly: AssemblyTitle("LedSdk 卸载程序")]
[assembly: AssemblyDescription("LedSdk 屏幕节目上传服务卸载程序")]
[assembly: AssemblyProduct("LedSdk")]
[assembly: AssemblyCompany("LedSdk")]
[assembly: AssemblyVersion("1.0.2.0")]
[assembly: AssemblyFileVersion("1.0.2.0")]

namespace LedSdkUninstaller
{
    internal sealed class UninstallOptions
    {
        public bool DeleteDesktop = true;
        public bool DeleteStartMenu = true;
        public bool DeleteAutoStart = true;
        public bool KeepUserData = true;
    }

    internal static class Program
    {
        internal const string Version = "1.0.2";
        private const int MoveFileDelayUntilReboot = 0x4;

        [DllImport("user32.dll")]
        private static extern bool SetProcessDPIAware();

        [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
        private static extern bool MoveFileEx(string existingFileName, string newFileName, int flags);

        [STAThread]
        private static int Main(string[] args)
        {
            try { SetProcessDPIAware(); } catch { }
            Application.EnableVisualStyles();
            Application.SetCompatibleTextRenderingDefault(false);

            string target = ArgValue(args, "--target");
            if (string.IsNullOrEmpty(target))
            {
                target = Path.GetDirectoryName(Application.ExecutablePath);
            }
            target = Path.GetFullPath(target);
            bool fromTemp = HasArg(args, "--from-temp");
            bool quiet = HasArg(args, "--quiet");

            // 从安装目录运行时先复制到临时目录，否则无法删除正在运行的自身。
            if (!fromTemp && IsInside(Application.ExecutablePath, target))
            {
                string temp = Path.Combine(
                    Path.GetTempPath(),
                    "LedSdkUninstall-" + Guid.NewGuid().ToString("N") + ".exe");
                File.Copy(Application.ExecutablePath, temp, true);
                string extra = "--from-temp --target \"" + target + "\"";
                if (quiet) extra += " --quiet";
                Process.Start(new ProcessStartInfo(temp, extra) { UseShellExecute = false });
                return 0;
            }

            if (quiet)
            {
                string log = Path.Combine(Path.GetTempPath(), "LedSdkUninstall.log");
                try { File.WriteAllText(log, ""); } catch { }
                List<string> errors;
                try
                {
                    errors = Uninstaller.Run(
                        target,
                        new UninstallOptions(),
                        delegate(string message) { AppendLog(log, message); });
                }
                catch (Exception ex)
                {
                    AppendLog(log, "卸载失败：" + ex);
                    errors = new List<string>();
                    errors.Add(ex.Message);
                }
                if (fromTemp) ScheduleSelfDelete(Application.ExecutablePath);
                return errors.Count == 0 ? 0 : 1;
            }

            using (UninstallForm form = new UninstallForm(target))
            {
                Application.Run(form);
                if (fromTemp) ScheduleSelfDelete(Application.ExecutablePath);
                return form.ExitCode;
            }
        }

        internal static void ScheduleSelfDelete(string path)
        {
            // 非管理员时 MOVEFILE_DELAY_UNTIL_REBOOT 可能失败；优先用隐藏 PowerShell 延迟删除。
            try
            {
                string powershell = Path.Combine(
                    Environment.GetFolderPath(Environment.SpecialFolder.Windows),
                    "System32",
                    "WindowsPowerShell",
                    "v1.0",
                    "powershell.exe");
                string quoted = path.Replace("'", "''");
                ProcessStartInfo info = new ProcessStartInfo(
                    powershell,
                    "-NoProfile -WindowStyle Hidden -Command \"Start-Sleep -Seconds 2; Remove-Item -LiteralPath '"
                    + quoted + "' -Force\"");
                info.UseShellExecute = false;
                info.CreateNoWindow = true;
                Process.Start(info);
                return;
            }
            catch { }
            try { MoveFileEx(path, null, MoveFileDelayUntilReboot); } catch { }
        }

        private static void AppendLog(string path, string message)
        {
            try
            {
                File.AppendAllText(
                    path,
                    DateTime.Now.ToString("yyyy-MM-dd HH:mm:ss") + " " + message + Environment.NewLine,
                    Encoding.UTF8);
            }
            catch { }
        }

        private static bool IsInside(string path, string directory)
        {
            string root = Path.GetFullPath(directory);
            if (!root.EndsWith(Path.DirectorySeparatorChar.ToString())) root += Path.DirectorySeparatorChar;
            return Path.GetFullPath(path).StartsWith(root, StringComparison.OrdinalIgnoreCase);
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
    }

    internal static class Uninstaller
    {
        private static readonly string[] UserDataDirectories =
        {
            "config", "export", "logs", "cache", "upload"
        };

        internal static string AppExe(string target)
        {
            return Path.Combine(target, "ledsdk-tauri.exe");
        }

        internal static List<Process> FindAppProcesses(string target)
        {
            List<Process> result = new List<Process>();
            string expected = Path.GetFullPath(AppExe(target));
            foreach (Process process in Process.GetProcessesByName("ledsdk-tauri"))
            {
                bool match = true;
                try
                {
                    string actual = Path.GetFullPath(process.MainModule.FileName);
                    match = string.Equals(actual, expected, StringComparison.OrdinalIgnoreCase);
                }
                catch
                {
                    // 读不到路径时按进程名处理；ledsdk-tauri 名称唯一。
                }
                if (match) result.Add(process);
            }
            return result;
        }

        internal static List<string> Run(string target, UninstallOptions options, Action<string> progress)
        {
            List<string> errors = new List<string>();
            List<Process> running = FindAppProcesses(target);
            if (running.Count > 0)
            {
                if (progress != null) progress("正在停止 LedSdk…");
                ForceStop(running, errors);
            }

            if (options.DeleteDesktop)
            {
                if (progress != null) progress("正在删除桌面快捷方式…");
                DeleteOwnedShortcut(Path.Combine(DesktopDirectory(), "LedSdk.lnk"), target, errors);
            }
            if (options.DeleteStartMenu)
            {
                if (progress != null) progress("正在删除开始菜单快捷方式…");
                DeleteOwnedShortcut(Path.Combine(StartMenuDirectory(), "LedSdk.lnk"), target, errors);
            }
            if (options.DeleteAutoStart)
            {
                if (progress != null) progress("正在删除开机自启动…");
                DeleteOwnedAutoStart(target, errors);
            }

            if (progress != null) progress("正在删除卸载信息…");
            DeleteUninstallRegistry(errors);

            if (progress != null) progress("正在删除程序文件…");
            DeleteProgramFiles(target, options.KeepUserData, errors);
            if (progress != null) progress("卸载完成。");
            return errors;
        }

        internal static void ForceStop(List<Process> processes, List<string> errors)
        {
            foreach (Process process in processes)
            {
                try
                {
                    process.Kill();
                    if (!process.WaitForExit(3000))
                    {
                        errors.Add("进程 " + process.Id + " 未在 3 秒内退出。");
                    }
                }
                catch (Exception ex)
                {
                    errors.Add("停止进程 " + process.Id + " 失败：" + ex.Message);
                }
            }
            Thread.Sleep(500);
        }

        private static void DeleteOwnedShortcut(string path, string target, List<string> errors)
        {
            if (!File.Exists(path)) return;
            try
            {
                string shortcutTarget = ReadShortcutTarget(path);
                if (string.IsNullOrEmpty(shortcutTarget)) return;
                if (string.Equals(
                    Path.GetFullPath(shortcutTarget),
                    Path.GetFullPath(AppExe(target)),
                    StringComparison.OrdinalIgnoreCase))
                {
                    File.Delete(path);
                }
            }
            catch (Exception ex)
            {
                errors.Add("删除快捷方式失败：" + path + "：" + ex.Message);
            }
        }

        private static string ReadShortcutTarget(string path)
        {
            Type shellType = Type.GetTypeFromProgID("WScript.Shell");
            if (shellType == null) return null;
            object shell = Activator.CreateInstance(shellType);
            object shortcut = shellType.InvokeMember(
                "CreateShortcut",
                BindingFlags.InvokeMethod,
                null,
                shell,
                new object[] { path });
            return (string)shortcut.GetType().InvokeMember(
                "TargetPath",
                BindingFlags.GetProperty,
                null,
                shortcut,
                null);
        }

        private static void DeleteOwnedAutoStart(string target, List<string> errors)
        {
            try
            {
                using (RegistryKey key = Registry.CurrentUser.OpenSubKey(
                    @"Software\Microsoft\Windows\CurrentVersion\Run", true))
                {
                    if (key == null) return;
                    string value = key.GetValue("LedSdkTauri") as string;
                    if (!string.IsNullOrEmpty(value)
                        && value.IndexOf(AppExe(target), StringComparison.OrdinalIgnoreCase) >= 0)
                    {
                        key.DeleteValue("LedSdkTauri", false);
                    }
                }
            }
            catch (Exception ex)
            {
                errors.Add("删除开机自启动失败：" + ex.Message);
            }
        }

        private static void DeleteUninstallRegistry(List<string> errors)
        {
            try
            {
                Registry.CurrentUser.DeleteSubKeyTree(
                    @"Software\Microsoft\Windows\CurrentVersion\Uninstall\LedSdk",
                    false);
            }
            catch (Exception ex)
            {
                errors.Add("删除卸载注册项失败：" + ex.Message);
            }
        }

        private static void DeleteProgramFiles(string target, bool keepUserData, List<string> errors)
        {
            if (!Directory.Exists(target)) return;
            if (!keepUserData)
            {
                try { Directory.Delete(target, true); }
                catch (Exception ex) { errors.Add("删除安装目录失败：" + target + "：" + ex.Message); }
                return;
            }

            foreach (string entry in Directory.GetFileSystemEntries(target))
            {
                string name = Path.GetFileName(entry);
                bool keep = false;
                foreach (string dir in UserDataDirectories)
                {
                    if (string.Equals(name, dir, StringComparison.OrdinalIgnoreCase))
                    {
                        keep = true;
                        break;
                    }
                }
                if (keep) continue;
                try
                {
                    if (Directory.Exists(entry)) Directory.Delete(entry, true);
                    else File.Delete(entry);
                }
                catch (Exception ex)
                {
                    errors.Add("删除失败：" + entry + "：" + ex.Message);
                }
            }
        }

        private static string DesktopDirectory()
        {
            return Environment.GetFolderPath(Environment.SpecialFolder.DesktopDirectory);
        }

        private static string StartMenuDirectory()
        {
            return Path.Combine(
                Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData),
                @"Microsoft\Windows\Start Menu\Programs");
        }
    }

    internal sealed class UninstallForm : Form
    {
        private readonly string target;
        private readonly CheckBox desktop;
        private readonly CheckBox startMenu;
        private readonly CheckBox autoStart;
        private readonly CheckBox keepData;
        private readonly Button uninstall;
        private readonly Button cancel;
        private readonly ProgressBar progress;
        private readonly Label status;
        private bool busy;

        internal int ExitCode = 0;

        internal UninstallForm(string target)
        {
            this.target = target;
            Text = "LedSdk 卸载程序";
            AutoScaleMode = AutoScaleMode.Dpi;
            AutoScaleDimensions = new SizeF(96F, 96F);
            ClientSize = new Size(720, 570);
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
            root.RowCount = 7;
            root.ColumnStyles.Add(new ColumnStyle(SizeType.Percent, 100F));
            root.RowStyles.Add(new RowStyle(SizeType.AutoSize));
            root.RowStyles.Add(new RowStyle(SizeType.AutoSize));
            root.RowStyles.Add(new RowStyle(SizeType.AutoSize));
            root.RowStyles.Add(new RowStyle(SizeType.Absolute, 54F));
            root.RowStyles.Add(new RowStyle(SizeType.Percent, 100F));
            root.RowStyles.Add(new RowStyle(SizeType.Absolute, 74F));
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
            title.Text = "卸载 LedSdk 屏幕节目上传服务";
            title.Font = new Font(Font.FontFamily, 16F, FontStyle.Bold);
            title.AutoSize = true;
            title.Margin = new Padding(0, 0, 0, 4);
            header.Controls.Add(title, 0, 0);

            Label version = new Label();
            version.Text = "版本 " + Program.Version + " · 卸载向导";
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
            pathLayout.ColumnCount = 2;
            pathLayout.RowCount = 1;
            pathLayout.ColumnStyles.Add(new ColumnStyle(SizeType.Absolute, 98F));
            pathLayout.ColumnStyles.Add(new ColumnStyle(SizeType.Percent, 100F));
            pathLayout.RowStyles.Add(new RowStyle(SizeType.Absolute, 34F));
            pathGroup.Controls.Add(pathLayout);

            Label pathLabel = new Label();
            pathLabel.Text = "当前目录：";
            pathLabel.AutoSize = true;
            pathLabel.Dock = DockStyle.Fill;
            pathLabel.TextAlign = ContentAlignment.MiddleLeft;
            pathLabel.Margin = new Padding(0, 0, 4, 0);
            pathLayout.Controls.Add(pathLabel, 0, 0);

            TextBox pathBox = new TextBox();
            pathBox.Text = target;
            pathBox.ReadOnly = true;
            pathBox.TabStop = false;
            pathBox.Dock = DockStyle.Fill;
            pathBox.Margin = new Padding(0, 4, 0, 4);
            pathLayout.Controls.Add(pathBox, 1, 0);

            GroupBox optionsGroup = new GroupBox();
            optionsGroup.Text = "卸载选项";
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
            optionLayout.RowCount = 4;
            optionLayout.ColumnStyles.Add(new ColumnStyle(SizeType.Percent, 100F));
            optionLayout.RowStyles.Add(new RowStyle(SizeType.AutoSize));
            optionLayout.RowStyles.Add(new RowStyle(SizeType.AutoSize));
            optionLayout.RowStyles.Add(new RowStyle(SizeType.AutoSize));
            optionLayout.RowStyles.Add(new RowStyle(SizeType.AutoSize));
            optionsGroup.Controls.Add(optionLayout);

            desktop = NewCheckBox("删除桌面快捷方式");
            startMenu = NewCheckBox("删除开始菜单快捷方式");
            autoStart = NewCheckBox("删除开机自启动");
            keepData = NewCheckBox("保留用户数据（配置、导出、日志、缓存、上传）");
            optionLayout.Controls.Add(desktop, 0, 0);
            optionLayout.Controls.Add(startMenu, 0, 1);
            optionLayout.Controls.Add(autoStart, 0, 2);
            optionLayout.Controls.Add(keepData, 0, 3);

            Label note = new Label();
            note.Text = "卸载前会检查程序是否正在运行；如正在运行，可选择强制停止。\r\n"
                + "取消勾选“保留用户数据”将同时删除配置、导出、日志、缓存和上传目录。";
            note.AutoSize = false;
            note.Dock = DockStyle.Fill;
            note.ForeColor = Color.Gray;
            note.TextAlign = ContentAlignment.TopLeft;
            note.Margin = new Padding(1, 0, 0, 0);
            root.Controls.Add(note, 0, 3);

            TableLayoutPanel statusLayout = new TableLayoutPanel();
            statusLayout.Dock = DockStyle.Fill;
            statusLayout.ColumnCount = 1;
            statusLayout.RowCount = 2;
            statusLayout.ColumnStyles.Add(new ColumnStyle(SizeType.Percent, 100F));
            statusLayout.RowStyles.Add(new RowStyle(SizeType.Absolute, 18F));
            statusLayout.RowStyles.Add(new RowStyle(SizeType.Percent, 100F));
            statusLayout.Margin = new Padding(0, 0, 0, 8);
            root.Controls.Add(statusLayout, 0, 5);

            progress = new ProgressBar();
            progress.Style = ProgressBarStyle.Marquee;
            progress.MarqueeAnimationSpeed = 30;
            progress.Dock = DockStyle.Fill;
            progress.Margin = new Padding(0, 0, 0, 6);
            progress.Visible = false;
            statusLayout.Controls.Add(progress, 0, 0);

            status = new Label();
            status.Text = "确认卸载选项后，点击「卸载」开始。";
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
            root.Controls.Add(buttons, 0, 6);

            uninstall = NewDialogButton("卸载");
            uninstall.Click += OnUninstall;
            buttons.Controls.Add(uninstall);

            cancel = NewDialogButton("取消");
            cancel.Click += delegate { Close(); };
            buttons.Controls.Add(cancel);

            AcceptButton = uninstall;
            CancelButton = cancel;
            FormClosing += OnFormClosing;
        }

        private CheckBox NewCheckBox(string text)
        {
            CheckBox box = new CheckBox();
            box.Text = text;
            box.Checked = true;
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

        private void OnFormClosing(object sender, FormClosingEventArgs e)
        {
            if (busy) e.Cancel = true;
        }

        private void OnUninstall(object sender, EventArgs e)
        {
            if (busy) return;

            // 先检查是否正在运行；用户确认后才允许强制停止。
            List<Process> running = Uninstaller.FindAppProcesses(target);
            if (running.Count > 0)
            {
                DialogResult answer = MessageBox.Show(
                    this,
                    "LedSdk 正在运行，是否强制停止？",
                    "LedSdk 卸载",
                    MessageBoxButtons.YesNo,
                    MessageBoxIcon.Warning);
                if (answer != DialogResult.Yes)
                {
                    status.Text = "已取消卸载。";
                    return;
                }
            }

            busy = true;
            uninstall.Enabled = false;
            cancel.Enabled = false;
            desktop.Enabled = false;
            startMenu.Enabled = false;
            autoStart.Enabled = false;
            keepData.Enabled = false;
            progress.Visible = true;
            status.Text = "正在卸载…";

            UninstallOptions options = new UninstallOptions();
            options.DeleteDesktop = desktop.Checked;
            options.DeleteStartMenu = startMenu.Checked;
            options.DeleteAutoStart = autoStart.Checked;
            options.KeepUserData = keepData.Checked;

            Thread thread = new Thread(delegate()
            {
                try
                {
                    List<string> errors = Uninstaller.Run(target, options, SetStatus);
                    BeginInvoke(new MethodInvoker(delegate()
                    {
                        progress.Visible = false;
                        busy = false;
                        if (errors.Count == 0)
                        {
                            status.Text = "卸载完成。";
                            MessageBox.Show(
                                this,
                                "LedSdk 已卸载。",
                                "卸载完成",
                                MessageBoxButtons.OK,
                                MessageBoxIcon.Information);
                            ExitCode = 0;
                            Close();
                        }
                        else
                        {
                            Reenable();
                            status.Text = "卸载未完全完成。";
                            MessageBox.Show(
                                this,
                                "卸载未完全完成：\r\n\r\n" + string.Join("\r\n", errors.ToArray()),
                                "卸载结果",
                                MessageBoxButtons.OK,
                                MessageBoxIcon.Warning);
                        }
                    }));
                }
                catch (Exception ex)
                {
                    BeginInvoke(new MethodInvoker(delegate()
                    {
                        progress.Visible = false;
                        busy = false;
                        Reenable();
                        status.Text = "卸载失败。";
                        MessageBox.Show(this, ex.Message, "卸载失败", MessageBoxButtons.OK, MessageBoxIcon.Error);
                    }));
                }
            });
            thread.SetApartmentState(ApartmentState.STA);
            thread.IsBackground = true;
            thread.Start();
        }

        private void Reenable()
        {
            uninstall.Enabled = true;
            cancel.Enabled = true;
            desktop.Enabled = true;
            startMenu.Enabled = true;
            autoStart.Enabled = true;
            keepData.Enabled = true;
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
