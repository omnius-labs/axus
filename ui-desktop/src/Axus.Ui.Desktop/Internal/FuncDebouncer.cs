using Omnius.Core.Base;

namespace Omnius.Axus.Ui.Desktop.Service.Internal;

public class FuncDebouncer<T> : AsyncDisposableBase
     where T : notnull
{
    private readonly Func<T, CancellationToken, Task> _callback;
    private T? _lastParam;
    private bool _pending = false;
    private bool _running = false;

    private Task? _currentTask;
    private CancellationTokenSource _cancellationTokenSource = new();
    private readonly object _lockObject = new();

    public FuncDebouncer(Func<T, CancellationToken, Task> callback)
    {
        _callback = callback;
    }

    protected override async ValueTask OnDisposeAsync()
    {
        _cancellationTokenSource.Cancel();

        if (_currentTask is not null) await _currentTask;
    }

    public bool IsRunning => _running;

    public void Signal(T param)
    {
        lock (_lockObject)
        {
            _lastParam = param;
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
                T? lastParam;

                lock (_lockObject)
                {
                    if (!_pending) return;
                    lastParam = _lastParam!;
                    _pending = false;
                }

                await _callback(lastParam, _cancellationTokenSource.Token).ConfigureAwait(false);
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
