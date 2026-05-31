#!/usr/bin/env node

const { spawn } = require("child_process");
const fs = require("fs");
const path = require("path");

const serverPath =
  process.env.ROBLOX_STUDIO_MCP_BIN ||
  path.resolve(__dirname, "../target/release/rbx-studio-mcp");

const allowedTools = new Set(["search_assets"]);

let child;
let childStartError;
let parentBuffer = "";
let childBuffer = "";

function writeJson(stream, message) {
  stream.write(`${JSON.stringify(message)}\n`);
}

function pumpLines(chunk, getBuffer, setBuffer, handler) {
  let buffer = getBuffer() + chunk.toString();
  while (true) {
    const newline = buffer.indexOf("\n");
    if (newline < 0) break;
    const raw = buffer.slice(0, newline).trim();
    buffer = buffer.slice(newline + 1);
    if (!raw) continue;
    try {
      handler(JSON.parse(raw));
    } catch (err) {
      writeJson(process.stdout, {
        jsonrpc: "2.0",
        error: { code: -32700, message: `Invalid JSON through search wrapper: ${err.message}` },
      });
    }
  }
  setBuffer(buffer);
}

function startChild() {
  if (child || childStartError) return child;
  if (!fs.existsSync(serverPath)) {
    childStartError = `Roblox Studio MCP binary not found: ${serverPath}`;
    return null;
  }

  child = spawn(serverPath, ["--stdio"], { stdio: ["pipe", "pipe", "pipe"] });
  child.stderr.on("data", (chunk) => process.stderr.write(chunk));
  child.on("error", (err) => {
    childStartError = `Failed to start Roblox Studio MCP: ${err.message}`;
    child = null;
  });
  child.on("exit", () => {
    child = null;
  });
  child.stdout.on("data", (chunk) => {
    pumpLines(
      chunk,
      () => childBuffer,
      (value) => {
        childBuffer = value;
      },
      handleChildMessage
    );
  });
  return child;
}

function handleParentMessage(message) {
  if (message?.method === "tools/call" && !allowedTools.has(message.params?.name)) {
    writeJson(process.stdout, {
      jsonrpc: "2.0",
      id: message.id,
      error: {
        code: -32601,
        message: `Search wrapper exposes only ${Array.from(allowedTools).join(", ")}`,
      },
    });
    return;
  }

  const activeChild = startChild();
  if (!activeChild || childStartError) {
    writeJson(process.stdout, {
      jsonrpc: "2.0",
      id: message.id,
      error: { code: -32000, message: childStartError || "Roblox Studio MCP child is unavailable" },
    });
    return;
  }
  writeJson(activeChild.stdin, message);
}

function handleChildMessage(message) {
  if (Array.isArray(message?.result?.tools)) {
    message.result.tools = message.result.tools.filter((tool) => allowedTools.has(tool.name));
  }
  writeJson(process.stdout, message);
}

process.stdin.on("data", (chunk) => {
  pumpLines(
    chunk,
    () => parentBuffer,
    (value) => {
      parentBuffer = value;
    },
    handleParentMessage
  );
});

process.on("exit", () => {
  if (child) child.kill();
});
