# Development Guide

## Building Releases

To create a new release:

1. Bump the version in [Cargo.toml](/Cargo.toml).
2. Rebuild to ensure nothing changes.
3. Commit with message "Bump version to 1.0.0" and push changes.
4. Tag the commit:

   ```sh
   git tag -a v1.0.0 -m "Release v1.0.0"
   git push origin v1.0.0
   ```

5. Pushing a `v*` tag automatically triggers the release workflow. The tag must match the `Cargo.toml` version (e.g., `v1.0.0` for `1.0.0`), otherwise the workflow fails before building anything. It will:
   - Create a new release
   - Build binaries for Linux, macOS, and Windows
   - Upload the binaries to the release
   - Publish the crate to crates.io

You can find the releases at: <https://github.com/tarolling/seiri/releases>

The workflow can also be run manually from the Actions tab with a tag as input. To test a release without publishing, run it manually with `dry-run` as the tag.
