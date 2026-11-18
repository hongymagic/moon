using System;
using System.Diagnostics;
using System.IO;
using System.IO.Compression;
using System.Reflection;
using System.Security.AccessControl;
using System.Security.Cryptography;
using System.Security.Principal;
using System.Windows.Forms;

namespace Moon.Setup
{
    internal static class Program
    {
        private static DirectoryInfo CreateWorkspace(string profile)
        {
            // The UI and its approved admin worker need the same view of the
            // payload. AppData can be redirected for packaged Windows callers.
            var inheritance = InheritanceFlags.ContainerInherit | InheritanceFlags.ObjectInherit;
            var security = new DirectorySecurity();
            security.SetAccessRuleProtection(true, false);
            using (var identity = WindowsIdentity.GetCurrent())
            {
                foreach (var sid in new[] {
                    identity.User,
                    new SecurityIdentifier(WellKnownSidType.LocalSystemSid, null),
                    new SecurityIdentifier(WellKnownSidType.BuiltinAdministratorsSid, null)
                })
                {
                    security.AddAccessRule(new FileSystemAccessRule(sid, FileSystemRights.FullControl,
                        inheritance, PropagationFlags.None, AccessControlType.Allow));
                }
            }

            var path = Path.Combine(profile, "Moon-Setup-" + Guid.NewGuid().ToString("N"));
            if (Directory.Exists(path) || File.Exists(path))
                throw new IOException("Unable to create a fresh Moon setup directory.");
            return Directory.CreateDirectory(path, security);
        }

        [STAThread]
        private static int Main(string[] args)
        {
            bool verifyOnly = args.Length == 1 &&
                String.Equals(args[0], "/VERIFYONLY", StringComparison.OrdinalIgnoreCase);
            if (!verifyOnly && args.Length != 0)
                return 87; // ERROR_INVALID_PARAMETER

            try
            {
                using (var stream = Assembly.GetExecutingAssembly().GetManifestResourceStream("Moon.Package.zip"))
                using (var sha = SHA256.Create())
                {
                    if (stream == null || !String.Equals(BitConverter.ToString(sha.ComputeHash(stream)).Replace("-", ""),
                        Package.Sha256, StringComparison.OrdinalIgnoreCase))
                        throw new InvalidDataException("The installer is damaged. Download a fresh copy of Moon.");

                    stream.Position = 0;
                    using (var archive = new ZipArchive(stream, ZipArchiveMode.Read))
                    {
                        if (archive.GetEntry("setup.ps1") == null)
                            throw new InvalidDataException("The installer is missing Moon setup.");
                        if (verifyOnly)
                            return 0;
                        return RunSetup(archive);
                    }
                }
            }
            catch (Exception error)
            {
                if (!verifyOnly)
                    MessageBox.Show(error.Message, "Moon Setup", MessageBoxButtons.OK, MessageBoxIcon.Error);
                return 1;
            }
        }

        private static int RunSetup(ZipArchive archive)
        {
            string profile = Environment.GetFolderPath(Environment.SpecialFolder.UserProfile);
            if (String.IsNullOrEmpty(profile) || !Path.IsPathRooted(profile))
                throw new IOException("Windows could not locate your user profile.");
            var workspace = CreateWorkspace(profile);
            try
            {
                archive.ExtractToDirectory(workspace.FullName);
                var start = new ProcessStartInfo {
                    FileName = Path.Combine(Environment.SystemDirectory, @"WindowsPowerShell\v1.0\powershell.exe"),
                    Arguments = "-NoLogo -NoProfile -NonInteractive -ExecutionPolicy RemoteSigned -STA -File \"" +
                        Path.Combine(workspace.FullName, "setup.ps1") + "\"",
                    // The daemon and Settings may outlive setup. Do not let
                    // them hold a handle to the temporary extraction directory.
                    WorkingDirectory = profile,
                    UseShellExecute = false,
                    CreateNoWindow = true
                };
                // Older release ZIPs accept the same argument-free invocation.
                start.EnvironmentVariables["MOON_SETUP_EXE_DIRECTORY"] = workspace.FullName;
                using (var setup = Process.Start(start))
                {
                    setup.WaitForExit();
                    if (setup.ExitCode != 0 && !File.Exists(Path.Combine(workspace.FullName, ".moon-setup-visible")))
                        MessageBox.Show("Windows PowerShell closed before Moon Setup opened (exit code " +
                            setup.ExitCode + "). If this PC is managed, ask your administrator whether it can run local PowerShell scripts.",
                            "Moon Setup", MessageBoxButtons.OK, MessageBoxIcon.Error);
                    return setup.ExitCode;
                }
            }
            finally
            {
                // If the window crashed mid-install, an independent worker may
                // still need these files to finish or restore the old desktop.
                if (workspace.GetFileSystemInfos(".moon-setup-worker-*").Length == 0)
                    workspace.Delete(true);
            }
        }
    }
}
