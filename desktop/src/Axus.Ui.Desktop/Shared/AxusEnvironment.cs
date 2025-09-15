namespace Omnius.Axus.Ui.Desktop.Shared;

public record AxusEnvironment
{
    public required string StorageDirectoryPath { get; init; }
    public required string StateDirectoryPath { get; init; }
    public required string LogsDirectoryPath { get; init; }
}
