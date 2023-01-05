FROM rust:1.66 as builder

RUN USER=root cargo new --bin dss
WORKDIR ./dss
COPY ./Cargo.lock ./Cargo.lock
COPY ./Cargo.toml ./Cargo.toml
RUN cargo build --release
RUN rm src/*.rs

ADD . ./

RUN rm ./target/release/deps/dss*
RUN cargo build --release


FROM debian:buster-slim

ARG APP=/usr/src/app

ENV APP_USER=appuser

RUN groupadd $APP_USER \
    && useradd -g $APP_USER $APP_USER \
    && mkdir -p ${APP}

COPY --from=builder /dss/target/release/dss ${APP}/dss

RUN chown -R $APP_USER:$APP_USER ${APP}

USER $APP_USER
WORKDIR ${APP}

CMD ["./dss"]
