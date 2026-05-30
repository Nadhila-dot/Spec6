import { chromium } from "playwright-core";

const auth = process.env.BRIGHTDATA_SCRAPING_BROWSER_AUTH;
const targetUrl = process.argv[2];

if (!auth) {
  console.error("BRIGHTDATA_SCRAPING_BROWSER_AUTH is required");
  process.exit(1);
}

if (!targetUrl) {
  console.error("Target URL is required");
  process.exit(1);
}

const endpoint = `wss://${auth}@brd.superproxy.io:9222`;
let browser;

try {
  browser = await chromium.connectOverCDP(endpoint);
  const page = await browser.newPage();
  await page.goto(targetUrl, {
    waitUntil: "domcontentloaded",
    timeout: 45000,
  });
  await page.waitForTimeout(1500);
  const title = await page.title();
  const markdown = await page.evaluate(() => {
    const titleText = document.title?.trim();
    const bodyText = document.body?.innerText?.trim() ?? "";
    if (!titleText) return bodyText;
    return `# ${titleText}\n\n${bodyText}`;
  });
  const html = await page.content();
  process.stdout.write(
    JSON.stringify({
      title,
      markdown,
      html,
    }),
  );
} catch (error) {
  console.error(error instanceof Error ? error.message : String(error));
  process.exit(1);
} finally {
  await browser?.close().catch(() => {});
}
