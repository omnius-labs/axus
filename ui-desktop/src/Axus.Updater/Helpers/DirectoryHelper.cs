
namespace Omnius.Axus.Launcher.Helpers;

public static class DirectoryHelper
{
    public static bool TryCreate(string path)
    {
        if (Directory.Exists(path)) return false;
        Directory.CreateDirectory(path);
        return true;
    }

    public static void CreateOrTruncate(string path)
    {
        if (Directory.Exists(path)) Directory.Delete(path, true);
        Directory.CreateDirectory(path);
    }
}
