# Development with Docker

## Prerequisites

- Docker
- Docker Compose

## Getting Started

1. Build and start the development container:

   ```bash
   docker-compose up -d
   ```

2. Enter the container:

   ```bash
   docker-compose exec dev bash
   ```

3. Inside the container, you can:
   - Build the project: `cargo build`
   - Run tests: `cargo test`
   - Run the application: `cargo run`

4. The project directory is mounted at `/app` in the container, so any changes you make on your host machine will be reflected inside the container.

## Building Releases

To create a new release:

1. Bump the version in [Cargo.toml](/Cargo.toml).
2. Rebuild to ensure nothing changes.
3. Commit and push changes.
4. Tag the commit:

   ```sh
   git tag -a v1.0.0 -m "Release v1.0.0"
   git push origin v1.0.0
   ```

5. The GitHub Actions workflow will automatically:
   - Create a new release
   - Build binaries for Linux, macOS, and Windows
   - Upload the binaries to the release

You can find the releases at: <https://github.com/tarolling/seiri/releases>
