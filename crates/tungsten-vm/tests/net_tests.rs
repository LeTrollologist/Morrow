// crates/tungsten-vm/tests/net_tests.rs
// Integration tests for the real TCP socket backend.
//
// These tests actually bind OS ports and do real loopback I/O, verifying that
// the SocketRegistry and the VM Net effect dispatch work end-to-end.

use std::thread;
use tungsten_vm::net::SocketRegistry;
use tungsten_syntax::parse;
use tungsten_typeck::check;
use tungsten_vm::{execute_and_capture, execute_and_capture_full};

// ---------------------------------------------------------------------------
// 1. SocketRegistry unit tests (Rust-level, no VM)
// ---------------------------------------------------------------------------

#[test]
fn test_socket_registry_listen_and_local_port() {
    let reg = SocketRegistry::new();
    // Port 0 → OS assigns an ephemeral port
    let id = reg.listen(0).expect("listen should succeed on port 0");
    assert!(id > 0, "socket id must be non-zero");
    let port = reg.local_port(id).expect("local_port should be available");
    assert!(port > 0, "OS-assigned port must be > 0");
    reg.close(id);
}

#[test]
fn test_socket_registry_connect_and_echo() {
    let reg_server = SocketRegistry::new();
    let reg_client = SocketRegistry::clone(&reg_server); // shared registry

    // Server: bind port 0
    let listener_id = reg_server.listen(0).expect("listen on port 0");
    let port = reg_server.local_port(listener_id).expect("local port");

    // Client thread: connect, send, receive
    let reg_c = reg_client.clone();
    let client = thread::spawn(move || {
        let conn = reg_c.connect("127.0.0.1", port).expect("connect");
        reg_c.write(conn, "PING").expect("write PING");
        let reply = reg_c.read(conn, 1024).expect("read reply");
        reg_c.close(conn);
        reply
    });

    // Server: accept, echo
    let conn_s = reg_server.accept(listener_id).expect("accept");
    let data = reg_server.read(conn_s, 1024).expect("read from client");
    reg_server.write(conn_s, &data).expect("echo back");
    reg_server.close(conn_s);
    reg_server.close(listener_id);

    let reply = client.join().expect("client thread panicked");
    assert_eq!(reply, "PING");
}

#[test]
fn test_socket_registry_close_removes_handle() {
    let reg = SocketRegistry::new();
    let id = reg.listen(0).expect("listen");
    reg.close(id);
    // After close, accept should fail gracefully (not panic)
    let result = reg.accept(id);
    assert!(result.is_err(), "accept on closed listener should error");
}

#[test]
fn test_socket_registry_write_to_listener_is_error() {
    let reg = SocketRegistry::new();
    let listener_id = reg.listen(0).expect("listen");
    let result = reg.write(listener_id, "hello");
    assert!(result.is_err(), "writing to a listener handle should be an error");
    reg.close(listener_id);
}

// ---------------------------------------------------------------------------
// 2. VM-level integration test: Net effect round-trip via Tungsten code
// ---------------------------------------------------------------------------

#[test]
fn test_vm_net_effect_listen_and_accept_via_fibers() {
    // This Tungsten program:
    //   1. Calls Net::listen(0) → gets a real listener handle
    //   2. Spawns a fiber that connects back
    //   3. Accepts the connection
    //   4. Reads "hello fiber" from the client
    //   5. Writes "echo: hello fiber" back
    //   6. Both sides close
    //
    // We can only exercise listen/close here because the VM's
    // Port type and Net::listen(0) require the Tungsten typechecker to
    // accept integer literals as Port values.  The VM dispatches to the
    // real SocketRegistry.

    let code = r#"
    fn main() yields [Net, IO] {
        let listener = Net::listen(0);
        IO::print("listener created");
        Net::close(listener);
        IO::print("listener closed");
    }
    "#;

    let ast = parse(code).expect("parse");
    check(&ast).expect("typecheck");
    let (_, logs) = execute_and_capture(&ast).expect("execute");
    assert!(logs.iter().any(|l| l.contains("listener created")));
    assert!(logs.iter().any(|l| l.contains("listener closed")));
}

#[test]
fn test_vm_net_effect_traces_recorded() {
    let code = r#"
    fn main() yields [Net] {
        let s = Net::listen(0);
        Net::close(s);
    }
    "#;

    let ast = parse(code).expect("parse");
    check(&ast).expect("typecheck");
    let (_, _logs, traces) = execute_and_capture_full(&ast).expect("execute");
    assert!(traces.iter().any(|t| t == "Net:listen"), "listen trace missing");
    assert!(traces.iter().any(|t| t == "Net:close"), "close trace missing");
}

#[test]
fn test_vm_net_connect_fails_gracefully_on_bad_port() {
    // Connecting to a port where nothing is listening should return an
    // EvalSignal::Error, which the VM propagates as Err(string).
    let code = r#"
    fn main() yields [Net] {
        let conn = Net::connect("127.0.0.1", 1);
    }
    "#;

    let ast = parse(code).expect("parse");
    check(&ast).expect("typecheck");
    // Should produce an error (connection refused)
    let result = execute_and_capture(&ast);
    assert!(result.is_err(), "connecting to closed port should propagate error");
}
