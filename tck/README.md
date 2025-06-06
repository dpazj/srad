# Dockerised TCK

```shell
cd srad/tck
docker build -t tck-docker --file Dockerfile.tck . --progress=plain 
docker run --rm -it --network=host tck-docker
```