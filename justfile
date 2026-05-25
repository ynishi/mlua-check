# [group('allow-agent')]
# Run all examples and verify exit 0.
run-examples:
    cargo run --example basic_lint
    cargo run --example with_vm
    cargo run --example luacats
