use serde_json::{json, Value};
use std::{
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, ChildStdout, Command, Stdio},
};

/// Keep the authoritative TypeScript validator warm across editor changes.
pub struct SchemaRuntime {
    child: Child,
    input: ChildStdin,
    output: BufReader<ChildStdout>,
}

impl SchemaRuntime {
    pub fn start(directory: &Path) -> Result<Self, String> {
        let mut child = Command::new("node")
            .args([
                "--input-type=module",
                "-e",
                include_str!("runtime-bridge.mjs"),
            ])
            .arg(directory)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|e| format!("Cannot start Node.js schema validator: {e}"))?;
        let input = child.stdin.take().unwrap();
        let output = BufReader::new(child.stdout.take().unwrap());
        Ok(Self {
            child,
            input,
            output,
        })
    }

    pub fn validate(&mut self, source: &str) -> Result<Value, String> {
        writeln!(self.input, "{}", json!({"source":source})).map_err(|e| e.to_string())?;
        self.input.flush().map_err(|e| e.to_string())?;
        let mut line = String::new();
        self.output
            .read_line(&mut line)
            .map_err(|e| e.to_string())?;
        serde_json::from_str(&line).map_err(|e| format!("Schema validator unavailable (requires Node.js 24+ and installed runtime dependencies): {e}"))
    }
}

impl Drop for SchemaRuntime {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Find the runtime in the containing monorepo; never combine adjacent schema files.
pub fn find_schema_runtime(file: &Path) -> Option<PathBuf> {
    file.ancestors().find_map(|directory| {
        let candidate = directory.join("packages/plato-schema");
        candidate
            .join("src/parse.ts")
            .is_file()
            .then_some(candidate)
    })
}
