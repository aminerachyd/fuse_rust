#!/bin/bash

echo "Removing previous etcd-server container..."
docker rm -f etcd-server

echo "Starting etcd-server container..."
docker run -d --name etcd-server \
   --publish 2379:2379 \
   --publish 2380:2380 \
   --env ALLOW_NONE_AUTHENTICATION=yes \
   quay.io/coreos/etcd:v3.6.10 \
   etcd \
   --advertise-client-urls http://0.0.0.0:2379 \
   --listen-client-urls http://0.0.0.0:2379
