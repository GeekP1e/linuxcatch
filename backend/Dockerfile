ARG RUST_BUILD_IMAGE=rust:1.94-slim-bookworm
FROM ${RUST_BUILD_IMAGE} AS build
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY content ./content
RUN cargo build --release --locked

FROM debian:bookworm-slim
COPY --from=build /app/target/release/linuxcatch-backend /usr/local/bin/linuxcatch-backend
ENV LISTEN_ADDR=0.0.0.0:3000
EXPOSE 3000
CMD ["linuxcatch-backend"]
