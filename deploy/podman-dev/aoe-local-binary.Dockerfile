# The CA-trusting workspace image with its aoe binary replaced by one built from a
# local checkout, so a change to aoe can be run inside a real CityHall workspace.
#
#   # build the binary (see up.sh) so that ./aoe exists in the context directory, then:
#   podman build -t cityhall/aoe:v1.22.0 -f aoe-local-binary.Dockerfile <dir containing aoe>
#
# Layering, so that nothing is built on top of an earlier copy of itself:
#   localhost/cityhall/aoe-base:v1.22.0  pristine reference image
#   localhost/cityhall/aoe-ca:v1.22.0    + Caddy's internal CA          (aoe-ca.Dockerfile)
#   cityhall/aoe:v1.22.0                 + the locally built aoe binary (this file)
ARG BASE=localhost/cityhall/aoe-ca:v1.22.0
FROM ${BASE}
USER root
COPY aoe /usr/local/bin/aoe
RUN chmod 0755 /usr/local/bin/aoe
USER aoe
