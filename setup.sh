#!/bin/bash

# 1. Create .gitignore
cat <<EOF > .gitignore
/target
**/*.rs.bk
.DS_Store
*.log
.env
EOF

# 2. Create Dockerfile for Render (Server Deployment)
cat <<EOF > Dockerfile
FROM rust:1.75-slim as builder
WORKDIR /usr/src/app
COPY . .
RUN cargo build --release --bin server

FROM debian:bookworm-slim
WORKDIR /app
RUN apt-get update && apt-get install -y libssl-dev ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /usr/src/app/target/release/server /app/server
EXPOSE 8080
CMD ["./server"]
EOF

# 3. Initialize Git if needed and commit
if [ ! -d .git ]; then
    git init
fi

git add .
git commit -m "alpha: initial setup with docker and gitignore"

echo "✅ Project prepared for Alpha!"
echo "Next steps:"
echo "1. Create a repo on GitHub."
echo "2. Run: git remote add origin <url> && git push -u origin master"
echo "3. Connect the repo to Render.com as a Web Service."

