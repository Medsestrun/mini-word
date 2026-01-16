//! Mini-Word CLI (for testing purposes only)
//! The main interface is through WASM bindings.

fn main() {
    println!("Mini-Word Text Editor Core");
    println!("==========================");
    println!();
    println!("This is a library crate. To use it:");
    println!();
    println!("  1. Build WASM: wasm-pack build --target web");
    println!("  2. Run web app: cd web && npm install && npm run dev");
    println!();
    println!("For testing the core library:");
    println!("  cargo test");
}
