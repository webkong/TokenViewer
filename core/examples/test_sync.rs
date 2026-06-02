use std::ffi::CString;

// Directly test the FFI functions
extern "C" {
    fn tt_init(db_path: *const std::os::raw::c_char) -> *mut std::ffi::c_void;
    fn tt_sync_all(handle: *mut std::ffi::c_void) -> *mut std::os::raw::c_char;
    fn tt_query_summary(handle: *mut std::ffi::c_void, from: *const std::os::raw::c_char, to: *const std::os::raw::c_char) -> *mut std::os::raw::c_char;
    fn tt_free_string(ptr: *mut std::os::raw::c_char);
    fn tt_destroy(handle: *mut std::ffi::c_void);
}

fn main() {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    let db_path = format!("{}/.tokenviewer/data.db", home);
    println!("Opening database at: {}", db_path);

    std::fs::create_dir_all(format!("{}/.tokenviewer", home)).unwrap();

    let c_path = CString::new(db_path).unwrap();
    let handle = unsafe { tt_init(c_path.as_ptr()) };
    if handle.is_null() {
        println!("ERROR: tt_init returned null!");
        return;
    }
    println!("Database opened successfully!");

    // Sync
    let result_ptr = unsafe { tt_sync_all(handle) };
    if !result_ptr.is_null() {
        let result = unsafe { std::ffi::CStr::from_ptr(result_ptr) }.to_str().unwrap_or("?");
        println!("Sync result: {}", result);
        unsafe { tt_free_string(result_ptr) };
    }

    // Query summary
    let from = CString::new("2020-01-01T00:00:00Z").unwrap();
    let to = CString::new("2030-01-01T00:00:00Z").unwrap();
    let summary_ptr = unsafe { tt_query_summary(handle, from.as_ptr(), to.as_ptr()) };
    if !summary_ptr.is_null() {
        let summary = unsafe { std::ffi::CStr::from_ptr(summary_ptr) }.to_str().unwrap_or("?");
        println!("Summary: {}", summary);
        unsafe { tt_free_string(summary_ptr) };
    }

    unsafe { tt_destroy(handle) };
    println!("Done!");
}
