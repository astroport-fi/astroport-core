FROM alpine:latest

WORKDIR /terra

ADD https://github.com/terra-money/core/releases/download/v2.6.1/terra_2.6.1_Linux_arm64.tar.gz /tmp/terra.tar.gz
RUN tar -xvf /tmp/terra.tar.gz -C /usr/local/bin

RUN apk add --no-cache bash jq vim
ADD .terra /root/.terra

# rest server
EXPOSE 1317
# grpc
EXPOSE 9090
# tendermint p2p
EXPOSE 26656
# tendermint rpc
EXPOSE 26657

CMD ["/usr/local/bin/terrad", "version"]