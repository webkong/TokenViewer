using System.Runtime.InteropServices;
using System.Text.Json;
using TokenViewerWindows.Models;

namespace TokenViewerWindows;

public sealed class CoreBridge : IDisposable
{
    private IntPtr _handle;
    private readonly JsonSerializerOptions _jsonOptions = new() { PropertyNameCaseInsensitive = true };

    private CoreBridge(IntPtr handle)
    {
        _handle = handle;
    }

    public static CoreBridge CreateDefault()
    {
        var home = Environment.GetFolderPath(Environment.SpecialFolder.UserProfile);
        var dbPath = Path.Combine(home, ".tokenviewer", "data.db");
        Directory.CreateDirectory(Path.GetDirectoryName(dbPath)!);
        var handle = tt_init(dbPath);
        return new CoreBridge(handle);
    }

    public bool IsReady => _handle != IntPtr.Zero;

    public UsageSummary? GetSummary(string from, string to)
    {
        var json = Call(h => tt_query_summary(h, from, to));
        return json is null ? null : JsonSerializer.Deserialize<UsageSummary>(json, _jsonOptions);
    }

    public ProviderStatus[] GetProviderStatus()
    {
        var json = Call(tt_get_provider_status);
        return json is null ? [] : JsonSerializer.Deserialize<ProviderStatus[]>(json, _jsonOptions) ?? [];
    }

    public string? SyncAll()
    {
        return Call(tt_sync_all);
    }

    public void Dispose()
    {
        if (_handle != IntPtr.Zero)
        {
            tt_destroy(_handle);
            _handle = IntPtr.Zero;
        }
    }

    private string? Call(Func<IntPtr, IntPtr> invoke)
    {
        if (_handle == IntPtr.Zero) return null;
        var ptr = invoke(_handle);
        if (ptr == IntPtr.Zero) return null;
        try
        {
            return Marshal.PtrToStringUTF8(ptr);
        }
        finally
        {
            tt_free_string(ptr);
        }
    }

    [DllImport("tokenviewer_core", CallingConvention = CallingConvention.Cdecl, CharSet = CharSet.Ansi)]
    private static extern IntPtr tt_init(string dbPath);

    [DllImport("tokenviewer_core", CallingConvention = CallingConvention.Cdecl)]
    private static extern IntPtr tt_query_summary(IntPtr handle, string from, string to);

    [DllImport("tokenviewer_core", CallingConvention = CallingConvention.Cdecl)]
    private static extern IntPtr tt_get_provider_status(IntPtr handle);

    [DllImport("tokenviewer_core", CallingConvention = CallingConvention.Cdecl)]
    private static extern IntPtr tt_sync_all(IntPtr handle);

    [DllImport("tokenviewer_core", CallingConvention = CallingConvention.Cdecl)]
    private static extern void tt_destroy(IntPtr handle);

    [DllImport("tokenviewer_core", CallingConvention = CallingConvention.Cdecl)]
    private static extern void tt_free_string(IntPtr ptr);
}

