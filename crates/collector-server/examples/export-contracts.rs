fn main() {
    println!(
        "{}",
        serde_json::to_string_pretty(&collector_server::contract_documents()).unwrap()
    );
}
