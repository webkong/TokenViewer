using System.Runtime.InteropServices;
using System.Text.Json;
using TokenViewerWindows.Models;

namespace TokenViewerWindows;

/// <summary>
/// Abstraction over the Rust core so the data-layer components can be tested
/// against a fake without touching the real database or FFI boundary.
/// </summary>
public interface ICoreBridge : IDisposable
{
    bool IsReady { get; }
    UsageSummary? GetSummary(string from, string to);
    DailyPoint[] GetDaily(string from, string to);
    DailyPoint[] GetHourly(string from, string to);
    ModelEntry[] GetModelBreakdown(string from, string to);
    HeatmapPoint[] GetHeatmap(int weeks);
    AgentStatus[] GetAgentStatus();
    SyncResult? SyncAll();
    SyncResult? RebuildAll();
}

public sealed class CoreBridge : ICoreBridge
{
    /// <summary>Shared JSON options. Property matching is case-insensitive; field
    /// names are mapped via explicit [JsonPropertyName("snake_case")] on each record.</summary>
    public static readonly JsonSerializerOptions JsonOptions = new() { PropertyNameCaseInsensitive = true };

    private IntPtr _handle;
    // Serializes every FFI call so the single Rust handle (and its SQLite
    // connection) is never entered concurrently from query and sync callers.
    private readonly object _gate = new();

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

    public UsageSummary? GetSummary(string from, string to) =>
        Deserialize<UsageSummary>(Call(h => tt_query_summary(h, from, to)));

    public DailyPoint[] GetDaily(string from, string to) =>
        Deserialize<DailyPoint[]>(Call(h => tt_query_daily(h, from, to))) ?? [];

    public DailyPoint[] GetHourly(string from, string to) =>
        Deserialize<DailyPoint[]>(Call(h => tt_query_hourly(h, from, to))) ?? [];

    public ModelEntry[] GetModelBreakdown(string from, string to) =>
        Deserialize<ModelEntry[]>(Call(h => tt_query_model_breakdown(h, from, to))) ?? [];

    public HeatmapPoint[] GetHeatmap(int weeks) =>
        Deserialize<HeatmapPoint[]>(Call(h => tt_query_heatmap(h, weeks))) ?? [];

    public AgentStatus[] GetAgentStatus() =>
        Deserialize<AgentStatus[]>(Call(h => tt_get_agent_status(h))) ?? [];

    public SyncResult? SyncAll() =>
        Deserialize<SyncResult>(Call(h => tt_sync_all(h)));

    public SyncResult? RebuildAll() =>
        Deserialize<SyncResult>(Call(h => tt_rebuild_all(h)));

    public void Dispose()
    {
        if (_handle != IntPtr.Zero)
        {
            tt_destroy(_handle);
            _handle = IntPtr.Zero;
        }
    }

    private static T? Deserialize<T>(string? json) =>
        json is null ? default : JsonSerializer.Deserialize<T>(json, JsonOptions);

    private string? Call(Func<IntPtr, IntPtr> invoke)
    {
        lock (_gate)
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
    }

    // All input strings are explicitly marshalled as UTF-8 (LPUTF8Str). The
    // Rust core reads database paths and range strings as UTF-8 C strings;
    // CharSet.Ansi would transcode through the system ANSI codepage and fail
    // for non-ASCII user/profile paths.
    [DllImport("tokenviewer_core", CallingConvention = CallingConvention.Cdecl)]
    private static extern IntPtr tt_init([MarshalAs(UnmanagedType.LPUTF8Str)] string dbPath);

    [DllImport("tokenviewer_core", CallingConvention = CallingConvention.Cdecl)]
    private static extern IntPtr tt_cursor_access_token([MarshalAs(UnmanagedType.LPUTF8Str)] string dbPath);

    [DllImport("tokenviewer_core", CallingConvention = CallingConvention.Cdecl)]
    private static extern int tt_kiro_has_login([MarshalAs(UnmanagedType.LPUTF8Str)] string dbPath);

    /// <summary>Reads the Cursor account access token from a VS Code
    /// <c>state.vscdb</c> SQLite database via the narrow Rust helper (read-only,
    /// fixed query). Returns null when the DB/key is missing or unreadable.</summary>
    public static string? ReadCursorAccessToken(string dbPath)
    {
        var ptr = tt_cursor_access_token(dbPath);
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

    /// <summary>Checks the Kiro CLI database for an actual login token. A saved
    /// device registration is intentionally not treated as a logged-in account.</summary>
    public static bool HasKiroLogin(string dbPath) => tt_kiro_has_login(dbPath) != 0;


    [DllImport("tokenviewer_core", CallingConvention = CallingConvention.Cdecl)]
    private static extern IntPtr tt_query_summary(
        IntPtr handle,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string from,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string to);

    [DllImport("tokenviewer_core", CallingConvention = CallingConvention.Cdecl)]
    private static extern IntPtr tt_query_daily(
        IntPtr handle,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string from,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string to);

    [DllImport("tokenviewer_core", CallingConvention = CallingConvention.Cdecl)]
    private static extern IntPtr tt_query_hourly(
        IntPtr handle,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string from,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string to);

    [DllImport("tokenviewer_core", CallingConvention = CallingConvention.Cdecl)]
    private static extern IntPtr tt_query_model_breakdown(
        IntPtr handle,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string from,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string to);

    [DllImport("tokenviewer_core", CallingConvention = CallingConvention.Cdecl)]
    private static extern IntPtr tt_query_heatmap(IntPtr handle, int weeks);

    [DllImport("tokenviewer_core", CallingConvention = CallingConvention.Cdecl)]
    private static extern IntPtr tt_get_agent_status(IntPtr handle);

    [DllImport("tokenviewer_core", CallingConvention = CallingConvention.Cdecl)]
    private static extern IntPtr tt_sync_all(IntPtr handle);

    [DllImport("tokenviewer_core", CallingConvention = CallingConvention.Cdecl)]
    private static extern IntPtr tt_rebuild_all(IntPtr handle);

    [DllImport("tokenviewer_core", CallingConvention = CallingConvention.Cdecl)]
    private static extern void tt_destroy(IntPtr handle);

    [DllImport("tokenviewer_core", CallingConvention = CallingConvention.Cdecl)]
    private static extern void tt_free_string(IntPtr ptr);
}
