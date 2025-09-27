using Omnius.Core.Base;

namespace Omnius.Axus.Ui.Desktop.Service.Internal;

public class ActionDebouncer : AsyncDisposableBase
{
    private readonly Func<CancellationToken, Task> _callback;
    private bool _pending = false;
    private bool _running = false;

    private Task? _currentTask;
    private CancellationTokenSource _cancellationTokenSource = new();
    private readonly object _lockObject = new();

    public ActionDebouncer(Func<CancellationToken, Task> callback)
    {
        _callback = callback;
    }

    protected override async ValueTask OnDisposeAsync()
    {
        _cancellationTokenSource.Cancel();

        if (_currentTask is not null) await _currentTask;
    }

    public bool IsRunning => _running;

    public void Signal()
    {
        lock (_lockObject)
        {
            _pending = true;

            if (_running) return;
            _running = true;
            _currentTask = this.RunAsync();
        }
    }

    private async Task RunAsync()
    {
        await Task.Delay(1).ConfigureAwait(false);

        try
        {
            for (; ; )
            {
                lock (_lockObject)
                {
                    if (!_pending) return;
                    _pending = false;
                }

                await _callback(_cancellationTokenSource.Token).ConfigureAwait(false);
            }
        }
        catch (OperationCanceledException)
        {
        }
        finally
        {
            lock (_lockObject)
            {
                _running = false;
            }
        }
    }
}
