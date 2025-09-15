using System.Diagnostics;
using System.IO.Compression;
using System.Runtime.InteropServices;
using Omnius.Axus.Launcher.Helpers;

namespace Omnius.Axus.Launcher;

public static class Replacer
{
    private static readonly NLog.Logger _logger = NLog.LogManager.GetCurrentClassLogger();

    public static bool TryReplace(string basePath)
    {
        try
        {
            string zipDirPath = Path.Combine(basePath, "update", "zip");
            string newDirPath = Path.Combine(basePath, "update", "new");

            var fileLock = new FileLock(Path.Combine(basePath, "bin", "lock"));

            using (fileLock.Lock(TimeSpan.FromSeconds(30)))
            {
                if (!Directory.Exists(zipDirPath)) return false;

                var zipFilePath = Directory.GetFiles(zipDirPath, "*.zip").FirstOrDefault();
                if (zipFilePath is null) return false;

                DirectoryHelper.CreateOrTruncate(newDirPath);
                ZipFile.ExtractToDirectory(zipFilePath, newDirPath);

                File.Delete(zipFilePath);

                string binDirPath = Path.Combine(basePath, "bin");
                string backupDirPath = Path.Combine(basePath, "backup");

                if (Directory.Exists(backupDirPath)) Directory.Delete(backupDirPath, true);

                MoveFilesForReplace(binDirPath, Path.Combine(backupDirPath, "bin"));
                CopyFilesForReplace(Path.Combine(newDirPath, "bin"), binDirPath);

                var binExePath = Path.Combine(binDirPath, "Omnius.Axus.Ui.Desktop");

                if (RuntimeInformation.IsOSPlatform(OSPlatform.Windows))
                {
                    binExePath += ".exe";
                }

                var processInfo = new ProcessStartInfo()
                {
                    FileName = binExePath,
                    WorkingDirectory = Path.GetDirectoryName(binExePath),
                    UseShellExecute = false,
                };

                Process.Start(processInfo);
            }

            return true;
        }
        catch (Exception e)
        {
            _logger.Error(e, $"Failed to Extract");
            return false;
        }
    }

    private static void MoveFilesForReplace(string sourcePath, string destinationPath)
    {
        DirectoryHelper.CreateOrTruncate(destinationPath);

        foreach (var dirPath in Directory.GetDirectories(sourcePath, "*", SearchOption.TopDirectoryOnly))
        {
            var dirName = Path.GetFileName(dirPath);
            Directory.Move(dirPath, Path.Combine(destinationPath, dirName));
        }

        foreach (var filePath in Directory.GetFiles(sourcePath, "*", SearchOption.TopDirectoryOnly))
        {
            var fileName = Path.GetFileName(filePath);
            if (fileName == "lock") continue;

            File.Move(filePath, Path.Combine(destinationPath, fileName));
        }
    }

    private static void CopyFilesForReplace(string sourcePath, string destinationPath)
    {
        foreach (string dirPath in Directory.GetDirectories(sourcePath, "*", SearchOption.AllDirectories))
        {
            Directory.CreateDirectory(dirPath.Replace(sourcePath, destinationPath, StringComparison.InvariantCulture));
        }

        foreach (string newPath in Directory.GetFiles(sourcePath, "*.*", SearchOption.AllDirectories))
        {
            File.Copy(newPath, newPath.Replace(sourcePath, destinationPath, StringComparison.InvariantCulture), true);
        }
    }
}
