fn main() {
    uniffi::generate_scaffolding("src/core.udl").expect("uniffi scaffolding");
}
