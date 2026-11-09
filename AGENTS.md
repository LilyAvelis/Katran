# Katran development rules

API First is mandatory, as explicitly required by the project owner.

Start with the current README and [development map](docs/development.md).
Documents shelf-02/shelf-03 are historical snapshots, not current launch instructions.
For parallel agents, assign disjoint module files; one integrator owns shared
Command/dispatcher/schema/catalogue edits. See the map for extension recipes.

The owner performs Windows UI checks herself. Do not use computer-use automation
or manipulate her desktop unless she explicitly asks. Verify API/code with tools
and provide a runnable build for manual visual feedback.

- Implement every product capability in the Rust domain/command API first.
- Verify behavior programmatically through that API before building its GUI.
- GUI, CLI, capture adapters and Afio integration are clients of one dispatcher.
  Never implement a separate action or storage path inside a visual component.
- GUI input method and presentation must not determine domain behavior.
- Keep Katran independent of Afio. Afio is a native integration client.
- Keep success/error semantics explicit, including per-item batch failures.
- Use synthetic data for tests; never silently overwrite or delete source files.
- Keep technical contracts, build instructions and limitations with the code.
