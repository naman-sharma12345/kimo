# Build kimo-mcp from source
FROM rust:1-bookworm AS build
WORKDIR /src
COPY . .
RUN cargo build --release -p kimo-mcp

# Runtime: kimo-mcp speaks MCP over stdio and needs ffmpeg/ffprobe on PATH
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ffmpeg ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=build /src/target/release/kimo-mcp /usr/local/bin/kimo-mcp
COPY --from=build /src/fonts /usr/local/share/kimo/fonts
ENTRYPOINT ["/usr/local/bin/kimo-mcp"]
