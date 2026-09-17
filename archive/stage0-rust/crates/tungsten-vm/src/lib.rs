pub mod effects;
pub mod eval;
pub mod net;
pub mod value;

use eval::Evaluator;
use tungsten_syntax::ast::Program;
use value::Value;

pub fn execute(program: &Program) -> Result<Value, String> {
    let mut vm = Evaluator::new();
    vm.load_program(program);
    vm.run_main()
}

pub fn execute_and_capture(program: &Program) -> Result<(Value, Vec<String>), String> {
    let mut vm = Evaluator::new();
    vm.load_program(program);
    let val = vm.run_main()?;
    let logs = vm.stdout_lines;
    Ok((val, logs))
}

pub fn execute_and_capture_full(program: &Program) -> Result<(Value, Vec<String>, Vec<String>), String> {
    let mut vm = Evaluator::new();
    vm.load_program(program);
    let val = vm.run_main()?;
    let logs = vm.stdout_lines;
    let traces = vm.effect_traces;
    Ok((val, logs, traces))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tungsten_syntax::parse;
    use tungsten_typeck::check;

    #[test]
    fn test_execute_player_with_effect_handlers() {
        let code = r#"
        type Health = u8(0..=100);

        struct Player {
            name: String,
            hp: Health,
        }

        fn fetch_player(id: u64) -> Player yields [Db, IOError] {
            let record = Db::query("SELECT * FROM players WHERE id = ?", id)?;
            
            Player {
                name: record.name,
                hp: record.hp as Health,
            }
        }

        fn heal_player(player: &mut Player, amount: u8) {
            player.hp = player.hp.saturating_add(amount);
        }

        fn main() {
            handle {
                let mut player = fetch_player(42)!;
                heal_player(&mut player, 20);
                println!("Player {} has {} HP", player.name, player.hp);
            } with Db {
                query(sql, args) => PostgresPool::execute(sql, args).await
            } with IOError {
                err => println!("Failed to fetch player: {}", err),
            }
        }
        "#;

        let ast = parse(code).expect("syntax parse ok");
        check(&ast).expect("typecheck ok");

        let (_, logs) = execute_and_capture(&ast).expect("runtime execution ok");
        assert_eq!(logs.len(), 1);
        // PostgresPool::execute returned hp: 80, heal_player added 20 -> saturating_add clamped to 100!
        assert_eq!(logs[0], "Player PlayerOne has 100 HP");
    }

    #[test]
    fn test_execute_generics_and_higher_order_fn() {
        let code = r#"
        struct Box<T> {
            val: T,
        }

        fn wrap<T>(item: T) -> Box<T> {
            Box { val: item }
        }

        fn apply<T, U>(val: T, f: fn(T) -> U) -> U {
            f(val)
        }

        fn double(n: i64) -> i64 {
            n * 2
        }

        fn slice_bounds(start: usize, end: usize(>= start)) -> usize {
            end - start
        }

        fn main() {
            let b = wrap(42);
            let doubled = apply(b.val, double);
            let span_len = slice_bounds(10, 25);
            println!("Doubled: {}, Span: {}", doubled, span_len);
        }
        "#;

        let ast = parse(code).expect("syntax parse ok");
        check(&ast).expect("typecheck ok");

        let (_, logs) = execute_and_capture(&ast).expect("runtime execution ok");
        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0], "Doubled: 84, Span: 15");
    }

    #[test]
    fn test_execute_fiber_concurrency_and_channels() {
        let code = r#"
        fn worker_task(ch: i64, value: i64) yields [Channel] {
            Channel::send(ch, value * 2);
        }

        fn main() yields [Async, Channel] {
            let ch = Channel::new();
            let f1 = Async::spawn(worker_task, ch, 21);
            let f2 = Async::spawn(worker_task, ch, 50);
            
            let res1 = Channel::recv(ch);
            let res2 = Channel::recv(ch);
            println!("Received fiber outputs: {} and {}", res1, res2);
        }
        "#;

        let ast = parse(code).expect("syntax parse ok");
        check(&ast).expect("typecheck ok");

        let (_, logs) = execute_and_capture(&ast).expect("runtime execution ok");
        assert_eq!(logs.len(), 1);
        assert!(logs[0].contains("42") && logs[0].contains("100"));
    }

    #[test]
    fn test_execute_nursery_structured_concurrency() {
        let code = r#"
        fn worker(ch: i64, value: i64) yields [Channel] {
            Channel::send(ch, value * 3);
        }

        fn main() yields [Async, Channel] {
            let ch = Channel::bounded(5);
            nursery n {
                n.spawn(worker, ch, 10);
                n.spawn(worker, ch, 20);
            }
            let r1 = Channel::recv(ch);
            let r2 = Channel::recv(ch);
            println!("Nursery results: {} and {}", r1, r2);
        }
        "#;

        let ast = parse(code).expect("syntax parse ok");
        check(&ast).expect("typecheck ok");

        let (_, logs) = execute_and_capture(&ast).expect("runtime execution ok");
        assert_eq!(logs.len(), 1);
        assert!(logs[0].contains("30") && logs[0].contains("60"));
    }

    #[test]
    fn test_execute_enums_pattern_matching_and_arrays() {
        let code = r#"
        enum Shape {
            Circle(i64),
            Rectangle(i64, i64),
            Point,
        }

        fn area(s: Shape) -> i64 {
            match s {
                Shape::Circle(r) => r * r * 3,
                Shape::Rectangle(w, h) => w * h,
                Shape::Point => 0,
            }
        }

        fn main() {
            let s1 = Shape::Circle(10);
            let s2 = Shape::Rectangle(4, 5);
            let s3 = Shape::Point;

            let a1 = area(s1);
            let a2 = area(s2);
            let a3 = area(s3);

            let arr = [a1, a2, a3];
            let sum = arr[0] + arr[1] + arr[2];

            println!("Areas: {}, {}, {}. Sum: {}", arr[0], arr[1], arr[2], sum);
        }
        "#;

        let ast = parse(code).expect("syntax parse ok");
        check(&ast).expect("typecheck ok");

        let (_, logs) = execute_and_capture(&ast).expect("runtime execution ok");
        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0], "Areas: 300, 20, 0. Sum: 320");
    }

    #[test]
    fn test_nursery_structured_join_guarantee() {
        // Verifies that the nursery waits for ALL spawned workers before
        // proceeding to Channel::recv — so no recv sees an empty channel.
        let code = r#"
        fn producer(ch: i64, x: i64) yields [Channel] {
            let v: i64 = x + 1;
            Channel::send(ch, v);
        }

        fn main() yields [Async, Channel] {
            let ch = Channel::bounded(4);
            nursery n {
                n.spawn(producer, ch, 10);
                n.spawn(producer, ch, 20);
                n.spawn(producer, ch, 30);
            }
            let a = Channel::recv(ch);
            let b = Channel::recv(ch);
            let c = Channel::recv(ch);
            let sum: i64 = a + b + c;
            println!("Join guarantee sum: {}", sum);
        }
        "#;
        // 11 + 21 + 31 = 63
        let ast = parse(code).expect("syntax ok");
        check(&ast).expect("typecheck ok");
        let (_, logs) = execute_and_capture(&ast).expect("runtime ok");
        assert_eq!(logs.len(), 1);
        assert!(logs[0].contains("63"), "expected 63, got: {}", logs[0]);
    }

    #[test]
    fn test_nursery_fan_out_pipeline() {
        // Simulates the HTTP-server fan-out pattern:
        //   N workers compute squares, parent sums all results.
        let code = r#"
        fn square(out: i64, value: i64) yields [Channel] {
            Channel::send(out, value * value);
        }

        fn main() yields [Async, Channel] {
            let results = Channel::bounded(4);
            nursery n {
                n.spawn(square, results, 3);
                n.spawn(square, results, 4);
                n.spawn(square, results, 5);
                n.spawn(square, results, 6);
            }
            let a = Channel::recv(results);
            let b = Channel::recv(results);
            let c = Channel::recv(results);
            let d = Channel::recv(results);
            let total: i64 = a + b + c + d;
            println!("Fan-out total: {}", total);
        }
        "#;
        // 9 + 16 + 25 + 36 = 86
        let ast = parse(code).expect("syntax ok");
        check(&ast).expect("typecheck ok");
        let (_, logs) = execute_and_capture(&ast).expect("runtime ok");
        assert_eq!(logs.len(), 1);
        assert!(logs[0].contains("86"), "expected 86, got: {}", logs[0]);
    }

    #[test]
    fn test_ffi_interpreter_graceful_error() {
        let code = r#"
        extern "C" {
            fn puts(s: *u8) -> i32;
        }

        fn main() {
            unsafe {
                let msg: *u8 = 0 as *u8;
                puts(msg);
            }
        }
        "#;
        let ast = parse(code).expect("syntax ok");
        check(&ast).expect("typecheck ok");
        let res = execute_and_capture(&ast);
        assert!(res.is_err());
        let err = res.unwrap_err();
        assert!(err.contains("cannot be run in the interpreter -- compile with 'forge build'"));
    }
}



