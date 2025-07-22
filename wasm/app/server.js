#!/usr/bin/env node

const express = require('express');
const cors = require('cors');
const path = require('path');

const app = express();
const port = 3000;

// Enable CORS for all routes and all origins
app.use(cors({
  origin: '*',
  methods: ['GET', 'POST', 'PUT', 'DELETE', 'OPTIONS'],
  allowedHeaders: '*',
  credentials: true
}));

// Serve static files from the current directory
app.use(express.static(__dirname));

// Serve the pkg directory from the parent directory
app.use('/pkg', express.static(path.join(__dirname, '../pkg')));

app.listen(port, () => {
  console.log(`🌐 Server running at http://localhost:${port}`);
  console.log(`📁 Serving files from: ${__dirname}`);
  console.log(`📦 WASM files from: ${path.join(__dirname, '../pkg')}`);
  console.log('✅ CORS enabled for all origins');
});