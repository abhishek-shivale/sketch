FROM rust:1-slim AS build
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo build --release

FROM debian:bookworm-slim
WORKDIR /app
COPY --from=build /app/target/release/sketch ./sketch
COPY public ./public
ENV BIND=0.0.0.0:3000
EXPOSE 3000
USER nobody
CMD ["./sketch"]
