pub mod effects;
pub mod eval;
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
}
