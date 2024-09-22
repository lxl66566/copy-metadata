# copy-metadata

cross-platform lib to copy metadata from one file to another.

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
