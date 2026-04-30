# copy-metadata

cross-platform lib to copy metadata from one file to another.

- TOCTOU-safe – all metadata changes are applied through open file handles.
- No symlink following – only regular files and directories are supported.
- Cross-platform – uses `fchmod`/`fchown`/`futimens` on Unix and `SetFileInformationByHandle`, `SetFileTime` on Windows.

## Usage

```sh
cargo add copy-metadata
```

functions:

```rust
fn copy_permission(from: impl AsRef<Path>, to: impl AsRef<Path>) -> io::Result<()>;
fn copy_time(from: impl AsRef<Path>, to: impl AsRef<Path>) -> io::Result<()>;
fn copy_metadata(from: impl AsRef<Path>, to: impl AsRef<Path>) -> io::Result<()>;
// copy_metadata = copy_permission + copy_time
```
