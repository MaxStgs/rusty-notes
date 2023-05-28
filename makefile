.PHONY: run build-dev build-prod run-dev run-prod stop-dev stop-prod up-db

IMAGE_NAME = notes-api-image
CONTAINER_NAME_DEV = notes-api-container-dev
CONTAINER_NAME_PROD = notes-api-container-prod

build-dev:
	docker build -t $(IMAGE_NAME):dev -f Dockerfile.dev .

build-prod:
	docker build -t $(IMAGE_NAME):prod -f Dockerfile.prod .

run-dev:
	docker run -d --name $(CONTAINER_NAME_DEV) -p 8080:8080 -v $(shell pwd):/usr/src/notes_api $(IMAGE_NAME):dev

run-prod:
	docker run -d --name $(CONTAINER_NAME_PROD) -p 8080:8080 $(IMAGE_NAME):prod

stop-dev:
	docker stop $(CONTAINER_NAME_DEV) && docker rm $(CONTAINER_NAME_DEV)

stop-prod:
	docker stop $(CONTAINER_NAME_PROD) && docker rm $(CONTAINER_NAME_PROD)
	
run:
	cargo watch -x run
	
up-db:
	docker run --name postgres-notes -e POSTGRES_USER=admin -e POSTGRES_PASSWORD=admin -e POSTGRES_DB=notes -p 5432:5432 -d postgres:13
	
connect-db:
	docker exec -it postgres-notes psql -U admin -d notes