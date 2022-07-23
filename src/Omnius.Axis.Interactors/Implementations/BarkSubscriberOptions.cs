namespace Omnius.Axis.Interactors;

public record BarkSubscriberOptions
{
    public BarkSubscriberOptions(string configDirectoryPath)
    {
        this.ConfigDirectoryPath = configDirectoryPath;
    }

    public string ConfigDirectoryPath { get; }
}
