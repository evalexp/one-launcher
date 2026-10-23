use one_launcher::everything::EverythingClient;

#[test]
fn test_everything_detection_and_query() {
    let client = EverythingClient::default();
    let is_running = client.is_available();
    println!("Everything 运行状态: {}", is_running);

    if is_running {
        // 如果 Everything 正在运行，测试实际 IPC 查询
        let start = std::time::Instant::now();
        let results = client.search("cargo.exe", 5);
        let elapsed = start.elapsed();

        println!("Everything IPC 耗时: {:?}, 查询结果条数: {:?}", elapsed, results.as_ref().map(|r| r.len()));

        assert!(results.is_ok(), "Everything 查询应成功返回: {:?}", results.err());
        let list = results.unwrap();
        if !list.is_empty() {
            let first = &list[0];
            println!("命中第一个文件: {} ({:?})", first.name, first.full_path);
            assert!(first.name.to_lowercase().contains("cargo.exe"));
        }
    }
}
