//! @author 十四叔
//! @date 2026/09/05

fn main() {
    if cfg!(target_os = "windows") {
        winresource::WindowsResource::new()
            .set_icon("assets/logo.ico")
            .set("FileDescription", "danqing-log")
            .set("ProductName", "danqing-log")
            .compile()
            .expect("编译 Windows 资源失败");
    }
}
