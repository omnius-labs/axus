using CommandLine;

namespace Omnius.Axus.Launcher;

public class Options
{
    [Option('b', "base")]
    public string? BasePath { get; set; } = null;
}
