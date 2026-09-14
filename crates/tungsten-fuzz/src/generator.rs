use crate::rng::Rng;

pub struct ProgramGenerator {
    rng: Rng,
    struct_defs: Vec<StructSchema>,
    next_var_id: usize,
    next_region_id: usize,
}

#[derive(Clone, Debug)]
struct StructSchema {
    name: String,
    fields: Vec<String>,
}

impl ProgramGenerator {
    pub fn new(rng: Rng) -> Self {
        Self {
            rng,
            struct_defs: Vec::new(),
            next_var_id: 0,
            next_region_id: 0,
        }
    }

    pub fn generate_valid_program(&mut self) -> String {
        let mut out = String::new();

        // 1. Generate Struct Definitions (1 to 3 structs)
        let num_structs = self.rng.gen_range(1, 3);
        self.struct_defs.clear();
        for s_idx in 0..num_structs {
            let s_name = format!("Schema{}", s_idx);
            let num_fields = self.rng.gen_range(1, 4);
            let mut field_names = Vec::new();
            out.push_str(&format!("struct {} {{\n", s_name));
            for f_idx in 0..num_fields {
                let f_name = format!("f{}", f_idx);
                out.push_str(&format!("    {}: i64,\n", f_name));
                field_names.push(f_name);
            }
            out.push_str("}\n\n");
            self.struct_defs.push(StructSchema {
                name: s_name,
                fields: field_names,
            });
        }

        // 2. Generate Helper Functions & Refinement Types
        out.push_str("type Percentage = u8(0..=100);\n\n");

        let s_ref = self.struct_defs[0].clone();
        out.push_str(&format!(
            "fn compute_struct_sum(p: &{}) -> i64 {{\n    ",
            s_ref.name
        ));
        let sum_expr = s_ref
            .fields
            .iter()
            .map(|f| format!("p.{}", f))
            .collect::<Vec<_>>()
            .join(" + ");
        out.push_str(&sum_expr);
        out.push_str("\n}\n\n");

        out.push_str("fn compute_scalar(v: i64) -> i64 {\n    v * 2 + 1\n}\n\n");

        // Helper testing refinement boundary conditions and JIT bounds-check elimination
        out.push_str("fn compute_refined_edge(val: i64) -> i64 {\n");
        out.push_str("    let p: Percentage = val as Percentage;\n");
        out.push_str("    p\n");
        out.push_str("}\n\n");

        // 3. Generate main() with regions and algebraic effect handling
        out.push_str("fn main() -> i64 yields [IO] {\n");
        let max_depth = self.rng.gen_range(1, 4);
        let body = self.generate_region_block(0, max_depth);
        out.push_str(&body);
        out.push_str("\n    handle {\n");
        out.push_str("        IO::print(\"completed-region-trace\");\n");
        out.push_str("    } with IO {\n");
        out.push_str("        print(msg) => 0\n");
        out.push_str("    };\n");
        out.push_str("    total\n");
        out.push_str("}\n");

        out
    }

    fn generate_region_block(&mut self, current_depth: usize, max_depth: usize) -> String {
        let mut code = String::new();
        let indent = "    ".repeat(current_depth + 1);

        let reg_name = format!("r{}", self.next_region_id);
        self.next_region_id += 1;

        code.push_str(&format!("{}let total = region {} {{\n", indent, reg_name));
        let inner_indent = "    ".repeat(current_depth + 2);

        // A. Allocate structs in this region
        let num_allocs = self.rng.gen_range(1, 3);
        let mut allocated_vars = Vec::new();

        for _ in 0..num_allocs {
            let schema = self.rng.choose(&self.struct_defs).clone();
            let var_name = format!("v{}", self.next_var_id);
            self.next_var_id += 1;

            code.push_str(&format!("{}let {} = {} {{\n", inner_indent, var_name, schema.name));
            for f in &schema.fields {
                let val = self.rng.gen_i64(1, 100);
                code.push_str(&format!("{}    {}: {},\n", inner_indent, f, val));
            }
            code.push_str(&format!("{}}};\n", inner_indent));
            allocated_vars.push((var_name, schema));
        }

        // B. Perform computation / helper function calls using references
        let (first_var, first_schema) = &allocated_vars[0];
        let acc_var = format!("acc{}", self.next_var_id);
        self.next_var_id += 1;

        code.push_str(&format!(
            "{}let mut {} = {}.{};\n",
            inner_indent, acc_var, first_var, first_schema.fields[0]
        ));

        // Use reference to struct if matching schema 0
        if first_schema.name == self.struct_defs[0].name {
            code.push_str(&format!(
                "{}{} = {} + compute_struct_sum(&{});\n",
                inner_indent, acc_var, acc_var, first_var
            ));
        }

        // C. Nested region or sibling region if depth permits
        if current_depth < max_depth {
            let nested_var = format!("sub_res{}", self.next_var_id);
            self.next_var_id += 1;

            let nested_code = self.generate_nested_region(current_depth + 1, max_depth);
            code.push_str(&format!("{}let {} = {};\n", inner_indent, nested_var, nested_code));
            code.push_str(&format!("{}{} = {} + {};\n", inner_indent, acc_var, acc_var, nested_var));
        }

        // Test refinement boundary math and algebraic effect invocation inside region
        let edge_val = if self.rng.gen_bool(0.33) {
            0
        } else if self.rng.gen_bool(0.5) {
            100
        } else {
            self.rng.gen_i64(1, 99)
        };
        code.push_str(&format!(
            "{}{} = {} + compute_refined_edge({});\n",
            inner_indent, acc_var, acc_var, edge_val
        ));
        code.push_str(&format!(
            "{}IO::print(\"region-progress\");\n",
            inner_indent
        ));

        // D. Trailing expression of the region (value, not reference!)
        code.push_str(&format!("{}{}\n", inner_indent, acc_var));
        code.push_str(&format!("{}}};", indent));

        code
    }

    fn generate_nested_region(&mut self, depth: usize, max_depth: usize) -> String {
        let reg_name = format!("sub_r{}", self.next_region_id);
        self.next_region_id += 1;

        let schema = self.rng.choose(&self.struct_defs).clone();
        let var_name = format!("sub_v{}", self.next_var_id);
        self.next_var_id += 1;

        let mut out = format!("region {} {{\n", reg_name);
        let indent = "    ".repeat(depth + 2);

        out.push_str(&format!("{}let {} = {} {{\n", indent, var_name, schema.name));
        for f in &schema.fields {
            let val = self.rng.gen_i64(1, 50);
            out.push_str(&format!("{}    {}: {},\n", indent, f, val));
        }
        out.push_str(&format!("{}}};\n", indent));

        // Mutable local computation
        out.push_str(&format!(
            "{}let mut local_sum = {}.{};\n",
            indent, var_name, schema.fields[0]
        ));

        // Recursive child if within depth
        if depth < max_depth && self.rng.gen_bool(0.5) {
            let child = self.generate_nested_region(depth + 1, max_depth);
            out.push_str(&format!("{}local_sum = local_sum + {};\n", indent, child));
        } else {
            out.push_str(&format!("{}local_sum = local_sum + compute_scalar(5);\n", indent));
        }

        out.push_str(&format!("{}local_sum\n", indent));
        out.push_str(&format!("{}}}", "    ".repeat(depth + 1)));

        out
    }
}
