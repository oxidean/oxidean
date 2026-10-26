# syntax=docker/dockerfile:1

FROM golang:1-alpine AS build
WORKDIR /app
COPY . .
RUN go build -o /bin/app .

FROM alpine:3
COPY --from=build /bin/app /bin/app
CMD ["/bin/app"]
