using System.Globalization;
using System.Reflection;
using CommandLine;

namespace Omnius.Axus.Launcher;

public class Program
{
    private static readonly NLog.Logger _logger = NLog.LogManager.GetCurrentClassLogger();

    public static async Task Main(string[] args)
    {
        var basePath = Directory.GetCurrentDirectory();
        SetLogsDirectory(Path.Combine(basePath, "../../storage/updater/logs"));

        _logger.Info("---- Start ----");
        _logger.Info(CultureInfo.InvariantCulture, "AssemblyInformationalVersion: {0}", Assembly.GetExecutingAssembly().GetCustomAttribute<AssemblyInformationalVersionAttribute>()?.InformationalVersion);

        AppDomain.CurrentDomain.UnhandledException += new UnhandledExceptionEventHandler(UnhandledException);

        try
        {
            var parsedResult = CommandLine.Parser.Default.ParseArguments<Options>(Environment.GetCommandLineArgs());
            parsedResult = parsedResult.WithParsed(options =>
            {
                // zipがダウンロード済みの場合、アップデートを実行する
                Replacer.TryReplace(options.BasePath!);
            });
        }
        catch (Exception e)
        {
            _logger.Error(e, $"Failed");
        }
        finally
        {
            _logger.Info("---- End ----");
            NLog.LogManager.Shutdown();
        }
    }

    private static void UnhandledException(object sender, UnhandledExceptionEventArgs e)
    {
        _logger.Error(e.ExceptionObject as Exception, "Unexpected Exception");
    }

    private static void SetLogsDirectory(string logsDirectoryPath)
    {
        var target = (NLog.Targets.FileTarget)NLog.LogManager.Configuration.FindTargetByName("log_file");
        target.FileName = $"{Path.GetFullPath(logsDirectoryPath)}/${{date:format=yyyy-MM-dd}}.log";
        target.ArchiveFileName = $"{Path.GetFullPath(logsDirectoryPath)}/logs/archive.{{#}}.log";
        NLog.LogManager.ReconfigExistingLoggers();
    }
}
