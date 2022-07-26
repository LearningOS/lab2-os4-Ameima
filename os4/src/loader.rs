// 获取链接到内核内的应用的数目
pub fn get_num_app() -> usize {
    // 从app的链接脚本link_app.S中获取符号，也就是通过build脚本构建的那个
    extern "C" {
        fn _num_app();
    }
    unsafe { (_num_app as usize as *const usize).read_volatile() }
}

// 根据传入的应用编号取出对应应用的 ELF 格式可执行文件数据。
pub fn get_app_data(app_id: usize) -> &'static [u8] {
    extern "C" {
        fn _num_app();
    }
    let num_app_ptr = _num_app as usize as *const usize;
    let num_app = get_num_app();
    let app_start = unsafe { core::slice::from_raw_parts(num_app_ptr.add(1), num_app + 1) };
    assert!(app_id < num_app);
    unsafe {
        // 利用link_app.S中已经放置好的符号，用app_id取出应用数据装到数组里
        core::slice::from_raw_parts(
            app_start[app_id] as *const u8,
            app_start[app_id + 1] - app_start[app_id],
        )
    }
}
