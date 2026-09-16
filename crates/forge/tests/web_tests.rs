use std::io::{Read, Write};
use std::net::TcpStream;
use std::process::{Child, Command};
use std::thread;
use std::time::Duration;

use forge::builder::{build_target, BuildOptions};

fn send_http_request(port: u16, raw_req: &str) -> String {
    let mut stream = None;
    for _ in 0..30 {
        if let Ok(s) = TcpStream::connect(format!("127.0.0.1:{}", port)) {
            stream = Some(s);
            break;
        }
        thread::sleep(Duration::from_millis(50));
    }
    let mut stream = stream.expect("Failed to connect to web service on 127.0.0.1");
    stream.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    stream.set_write_timeout(Some(Duration::from_secs(5))).unwrap();

    stream.write_all(raw_req.as_bytes()).expect("Write to server failed");

    let mut response = String::new();
    stream.read_to_string(&mut response).expect("Read from server failed");
    response
}

#[test]
fn test_web_service_full_lifecycle() {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();
    let src_path = root.join("examples").join("web_service.tg");
    assert!(src_path.exists(), "web_service.tg must exist at {}", src_path.display());

    let options = BuildOptions {
        release: false,
        emit_llvm: false,
        emit_asm: false,
        custom_out: None,
        target: None,
    };

    let target_str = src_path.to_str().unwrap();
    let exe_path = build_target(Some(target_str), &options).expect("Web service build should succeed");
    assert!(exe_path.exists(), "Built executable must exist at {}", exe_path.display());

    // Spawn the web service
    let mut child: Child = Command::new(&exe_path)
        .spawn()
        .expect("Failed to spawn web_service child process");

    // Allow the server a moment to bind the socket
    thread::sleep(Duration::from_millis(300));

    let port: u16 = 8088;

    // 1. Test GET /health
    let req_health = "GET /health HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n";
    let resp_health = send_http_request(port, req_health);
    assert!(resp_health.contains("HTTP/1.1 200 OK"), "Expected 200 OK for /health, got: {}", resp_health);
    assert!(resp_health.contains("application/json"), "Expected application/json header");
    assert!(resp_health.contains("tungsten-fortress/1.0"), "Expected server name in body");

    // 2. Test GET /users (initial seeded data)
    let req_users = "GET /users HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n";
    let resp_users = send_http_request(port, req_users);
    assert!(resp_users.contains("HTTP/1.1 200 OK"), "Expected 200 OK for /users, got: {}", resp_users);
    assert!(resp_users.contains("Alice"), "Expected Alice in users list");
    assert!(resp_users.contains("Bob"), "Expected Bob in users list");

    // 3. Test POST /users (Prepared Statement insertion)
    let req_post_user = "POST /users HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
    let resp_post = send_http_request(port, req_post_user);
    assert!(resp_post.contains("HTTP/1.1 201 Created"), "Expected 201 Created for POST /users, got: {}", resp_post);
    assert!(resp_post.contains("rows_affected"), "Expected rows_affected in response");

    // 4. Test GET /users again (verify persistence of new user)
    let resp_users_after = send_http_request(port, req_users);
    assert!(resp_users_after.contains("NewUser"), "Expected NewUser in updated users list");
    assert!(resp_users_after.contains("newuser@tungsten.lang"), "Expected new user email");

    // 5. Test GET /benchmark
    let req_bench = "GET /benchmark HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n";
    let resp_bench = send_http_request(port, req_bench);
    assert!(resp_bench.contains("HTTP/1.1 200 OK"), "Expected 200 OK for /benchmark, got: {}", resp_bench);
    assert!(resp_bench.contains("grand_web"), "Expected grand_web payload");

    // 6. Test 404 on nonexistent route
    let req_404 = "GET /nonexistent HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n";
    let resp_404 = send_http_request(port, req_404);
    assert!(resp_404.contains("HTTP/1.1 404 Not Found"), "Expected 404 Not Found, got: {}", resp_404);

    // 7. Send graceful /shutdown
    let req_shutdown = "GET /shutdown HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n";
    let resp_shutdown = send_http_request(port, req_shutdown);
    assert!(resp_shutdown.contains("shutting_down"), "Expected shutting_down in response");

    // Wait for process termination
    let status = child.wait().expect("Failed to wait on child process");
    assert!(status.success(), "Web service should exit cleanly with success code");
}
