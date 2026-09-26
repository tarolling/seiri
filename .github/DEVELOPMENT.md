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

5. Run the GitHub Actions workflow with the appropriate tag as input (e.g., v1.0.0). The workflow will:
   - Create a new release
   - Build binaries for Linux, macOS, and Windows
   - Upload the binaries to the release

You can find the releases at: <https://github.com/tarolling/seiri/releases>

If you want to test a release, pass in `dry-run` as the tag.
