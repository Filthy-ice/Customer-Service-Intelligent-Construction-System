fn main() {
    // ui/ 无显式追踪时改动不会触发重嵌：asset 以压缩形式入二进制，改一处即全量重打。
    println!("cargo:rerun-if-changed=ui");
    tauri_build::build()
}
