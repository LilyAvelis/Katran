use avilon_actions::{Assignment, Definition, Overrides, binding, plan_set};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let actions = [Definition {
        id: "refresh",
        label: "Refresh",
        default_binding: "Ctrl R",
        rebindable: true,
    }];
    let overrides = plan_set(
        &actions,
        &Overrides::new(),
        "refresh",
        Assignment::Set(Some("Alt R")),
        false,
        |_, _| false,
    )?;
    println!("Refresh: {}", binding(&actions, &overrides, "refresh"));
    Ok(())
}
