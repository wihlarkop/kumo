#![cfg(feature = "postgres")]

use kumo::store::PostgresStore;

#[test]
fn add_column_accepts_valid_names_without_connecting() {
    let builder = PostgresStore::builder("postgres://user:pass@localhost/db")
        .table("kumo_items")
        .create_table(false)
        .add_column("my_table_123", "TEXT");
    assert!(builder.is_ok());
}

#[test]
fn add_column_rejects_empty_name() {
    let builder =
        PostgresStore::builder("postgres://user:pass@localhost/db").add_column("", "TEXT");
    assert!(builder.is_err());
}

#[test]
fn add_column_rejects_names_over_63_chars() {
    let long = "a".repeat(64);
    let builder =
        PostgresStore::builder("postgres://user:pass@localhost/db").add_column(long, "TEXT");
    assert!(builder.is_err());
}

#[test]
fn add_column_rejects_sql_injection_names() {
    for name in [
        "drop table;--",
        "items; DROP TABLE users;--",
        "my-table",
        "my table",
    ] {
        let builder =
            PostgresStore::builder("postgres://user:pass@localhost/db").add_column(name, "TEXT");
        assert!(builder.is_err(), "{name} should be rejected");
    }
}
