// Independent browser accounting oracle; snapshots remain disposable.
const fs = require('node:fs');
const client = require('../../../../../../tests/client_harness');
const input = JSON.parse(fs.readFileSync(0, 'utf8'));
const app = client();
app.run(`world={progression:${JSON.stringify(input.catalog)}}`);
process.stdout.write(JSON.stringify(input.towns.map(town =>
  app.run(`resourceStocks(${JSON.stringify(town)})`))));
