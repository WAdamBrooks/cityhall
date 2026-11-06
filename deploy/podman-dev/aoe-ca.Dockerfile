# The reference aoe workspace image plus Caddy's internal root CA, so aoe can
# fetch its config bundle from https://cityhall inside the podman network.
#
#   podman tag cityhall/aoe:v1.22.0 localhost/cityhall/aoe-base:v1.22.0   # once, keeps a pristine copy
#   podman build --build-arg BASE=localhost/cityhall/aoe-base:v1.22.0 -t cityhall/aoe:v1.22.0 \
#     -f aoe-ca.Dockerfile <dir containing root.crt>
#
# Rebuilt in place under the same tag so CityHall's image template
# (cityhall/aoe:{version}) needs no change.
ARG BASE=localhost/cityhall/aoe-base:v1.22.0
FROM ${BASE}
USER root
COPY root.crt /usr/local/share/ca-certificates/cityhall-internal-ca.crt
RUN update-ca-certificates
USER aoe
