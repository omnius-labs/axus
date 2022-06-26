using Generator.Equals;
using Omnius.Axis.Interactors.Models;
using Omnius.Core.Avalonia;

namespace Omnius.Axis.Ui.Desktop.Windows.Main;

[Equatable]
public partial class UploadingFileViewModel : BindableBase, ICollectionViewModel<UploadingFileViewModel, UploadingFileReport>
{
    private UploadingFileReport? _model;

    public void Update(UploadingFileReport? model)
    {
        this.Model = model;
    }

    public UploadingFileReport? Model
    {
        get => _model;
        set
        {
            this.SetProperty(ref _model, value);
            this.RaisePropertyChanged(null);
        }
    }

    public string Name => Path.GetFileName(this.Model?.FilePath ?? "");

    public string FilePath => this.Model?.FilePath ?? "";

    public DateTime CreatedTime => this.Model?.CreatedTime ?? DateTime.MinValue;

    public UploadingFileState State => this.Model?.State ?? UploadingFileState.Unknown;
}
