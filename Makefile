build:
	sudo docker build -t dss -f docker/Dockerfile .
run:
	sudo docker run -i dss
go:
	sudo docker build -t dss -f docker/Dockerfile . && sudo docker run -i dss
test:
	sudo docker build -t dss-test -f docker/Dockerfile.test . && sudo docker run -i dss-test
