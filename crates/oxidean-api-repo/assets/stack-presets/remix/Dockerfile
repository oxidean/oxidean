# syntax=docker/dockerfile:1

FROM node:26-alpine AS deps
WORKDIR /app
COPY package.json ./
RUN npm install

FROM deps AS build
COPY . .
RUN npm run build

FROM node:26-alpine AS runner
WORKDIR /app
ENV NODE_ENV=production PORT=3000
COPY package.json ./
RUN npm install --omit=dev
COPY --from=build /app/build ./build
EXPOSE 3000
CMD ["npm", "run", "start"]
