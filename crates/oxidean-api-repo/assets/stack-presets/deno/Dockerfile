# syntax=docker/dockerfile:1

FROM denoland/deno:alpine-2.9.8
WORKDIR /app
COPY . .
EXPOSE 8000
CMD ["deno", "run", "--allow-net", "--allow-env", "main.ts"]
