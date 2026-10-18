use lantern_core::units::Quantity;
use lantern_host::resources::*;
use serde_json::Value;
use std::process::Command;

fn q(value: f64) -> Quantity {
    Quantity::new(value).unwrap()
}
fn key(name: &str) -> ResourceKey {
    ResourceKey::parse(name).unwrap()
}
fn stock(value: &Value) -> Stock {
    Stock::from_entries(
        value
            .as_object()
            .unwrap()
            .iter()
            .map(|(name, amount)| (key(name), q(amount.as_f64().unwrap()))),
    )
    .unwrap()
}

#[test]
fn reviewed_exports_match_current_python_without_writing() {
    assert!(
        Command::new("python3")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/fixtures/resources/generate.py"
            ))
            .arg("--check")
            .status()
            .unwrap()
            .success()
    );
}

#[test]
fn python_affordability_and_fractional_debits_match() {
    let cases: Value =
        serde_json::from_str(include_str!("fixtures/resources/python.json")).unwrap();
    for case in cases.as_array().unwrap() {
        let mut inventory = stock(&case["stock"]);
        let costs = stock(&case["costs"]);
        let affordable = case["affordable"].as_bool().unwrap();
        assert_eq!(inventory.can_afford(&costs), affordable, "{case}");
        assert_eq!(inventory.spend(&costs).is_ok(), affordable, "{case}");
        assert_eq!(inventory, stock(&case["after"]), "{case}");
    }
}

#[test]
fn catalog_preserves_all_resource_references_and_species() {
    let c = public_catalog();
    for field in [
        "storage_keys",
        "tradable_goods",
        "sellable_goods",
        "buyable_goods",
        "manual_sellable_goods",
    ] {
        for name in c[field].as_array().unwrap() {
            let name = name.as_str().unwrap();
            if name != "food" {
                key(name);
            }
        }
    }
    for field in [
        "edible",
        "sale_prices",
        "production_seconds",
        "fish",
        "smoked_fish",
        "crops",
    ] {
        for name in c[field].as_object().unwrap().keys() {
            key(name);
        }
    }
    for recipes in c["profession_recipes"].as_object().unwrap().values() {
        for recipe in recipes.as_array().unwrap() {
            key(recipe["output"].as_str().unwrap());
            for input in recipe["inputs"].as_object().unwrap().keys() {
                key(input);
            }
        }
    }
    for recipe in c["recipes"].as_object().unwrap().values() {
        for input in recipe[0].as_object().unwrap().keys() {
            key(input);
        }
        key(recipe[1].as_str().unwrap());
    }
    assert_eq!(key("trout").food_value(), 1.5);
    assert_eq!(key("smoked_trout").food_value(), 2.0);
    assert_eq!(key("smoked_trout").sale_price(), 3.0);
    assert_eq!(key("iron").sale_price(), 10.0);
    assert_eq!(key("wood").food_value(), 0.0);
    assert_eq!(key("wood").sale_price(), 1.0);
    assert_eq!(
        ResourceKey::parse("food"),
        Err(ResourceError::AggregateFood)
    );
    for invalid in ["", "Wood", "unknown", "trout "] {
        assert_eq!(
            ResourceKey::parse(invalid),
            Err(ResourceError::UnknownResource)
        );
    }
}

#[test]
fn invalid_and_duplicate_quantities_cannot_enter_stock() {
    for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -0.001] {
        assert!(Quantity::new(value).is_err());
    }
    assert_eq!(
        Stock::from_entries([(key("wood"), q(0.1)), (key("wood"), q(0.2))]),
        Err(ResourceError::DuplicateResource)
    );
    assert_eq!(Stock::default().quantity(&key("coins")), q(0.0));
    let mut inventory = Stock::default();
    inventory.credit(key("coins"), q(0.125)).unwrap();
    inventory.credit(key("coins"), q(0.125)).unwrap();
    assert_eq!(inventory.quantity(&key("coins")), q(0.25));
}

#[test]
fn transfers_conserve_fractional_species_stock_and_never_use_spend_tolerance() {
    let mut stored =
        Stock::from_entries([(key("trout"), q(1.25)), (key("salmon"), q(0.75))]).unwrap();
    let mut cargo = Stock::default();
    for amount in [0.125, 0.25, 0.875] {
        stored
            .transfer_to(&mut cargo, &key("trout"), q(amount))
            .unwrap();
        assert_eq!(
            stored.quantity(&key("trout")).get() + cargo.quantity(&key("trout")).get(),
            1.25
        );
        assert_eq!(stored.quantity(&key("salmon")), q(0.75));
    }
    let before = (stored.clone(), cargo.clone());
    assert_eq!(
        stored.transfer_to(&mut cargo, &key("trout"), q(5e-10)),
        Err(ResourceError::InsufficientStock)
    );
    assert_eq!((stored.clone(), cargo.clone()), before);
    cargo
        .transfer_to(&mut stored, &key("trout"), q(1.25))
        .unwrap();
    assert_eq!(stored.quantity(&key("trout")), q(1.25));
    assert_eq!(cargo.quantity(&key("trout")), q(0.0));
}

#[test]
fn overflow_is_atomic_for_credit_and_transfer() {
    let mut destination = Stock::from_entries([(key("wood"), q(f64::MAX))]).unwrap();
    let mut source = destination.clone();
    let before = destination.clone();
    assert_eq!(
        destination.credit(key("wood"), q(f64::MAX)),
        Err(ResourceError::Overflow)
    );
    assert_eq!(destination, before);
    assert_eq!(
        source.transfer_to(&mut destination, &key("wood"), q(f64::MAX)),
        Err(ResourceError::Overflow)
    );
    assert_eq!(destination, before);
    assert_eq!(source, before);
}
