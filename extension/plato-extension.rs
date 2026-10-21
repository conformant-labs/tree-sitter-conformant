use zed_extension_api::{self as zed, settings::LspSettings, LanguageServerId, Result};

struct PlatoExtension;

impl zed::Extension for PlatoExtension {
    fn new() -> Self {
        Self
    }

    fn language_server_command(
        &mut self,
        server_id: &LanguageServerId,
        worktree: &zed::Worktree,
    ) -> Result<zed::Command> {
        let binary = LspSettings::for_worktree(server_id.as_ref(), worktree)
            .ok()
            .and_then(|settings| settings.binary);
        let command = binary.as_ref().and_then(|binary| binary.path.clone())
            .or_else(|| worktree.which("plato-language-server"))
            .ok_or("Install the Plato folding server: cargo install --locked --path crates/plato-language-server (from tree-sitter-conformant)")?;
        Ok(zed::Command {
            command,
            args: binary
                .and_then(|binary| binary.arguments)
                .unwrap_or_default(),
            env: vec![],
        })
    }
}

zed::register_extension!(PlatoExtension);
