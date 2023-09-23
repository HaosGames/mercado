#!/bin/bash

mkdir ${PWD}/data
docker run --detach --publish 5000:5000 --name lnbits --volume ${PWD}/.env:/app/.env --volume ${PWD}/data/:/app/data lnbitsdocker/lnbits-legend
echo "Waiting for container and lnbits to start"
sleep 3s
docker exec lnbits cp /app/.super_user /app/data/.super_user
