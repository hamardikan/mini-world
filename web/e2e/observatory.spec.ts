import { expect, test } from '@playwright/test';

test('live observatory renders, selects agents, and applies one SSE step', async ({ page }) => {
  await page.emulateMedia({ reducedMotion: 'reduce' });
  const eventsRequest = page.waitForRequest((request) => request.url().includes('/v1/events'));
  await page.goto('/');
  await eventsRequest;
  await expect(page.locator('.connection-value.state-live')).toHaveText(/Live/);
  await expect(page.locator('.world-canvas')).toBeVisible();
  const canvasEvidence = await page.locator('canvas.world-canvas').evaluate((canvas) => {
    if (canvas.width !== 512 || canvas.height !== 512) return { painted: false, width: canvas.width, height: canvas.height };
    const context = canvas.getContext('2d');
    if (!context) return { painted: false, width: canvas.width, height: canvas.height };
    const pixels = context.getImageData(0, 0, canvas.width, canvas.height).data;
    let nonBlank = 0;
    for (let index = 0; index < pixels.length; index += 4) {
      if (pixels[index] !== 0 || pixels[index + 1] !== 0 || pixels[index + 2] !== 0 || pixels[index + 3] !== 0) nonBlank += 1;
    }
    return { painted: nonBlank > 512, width: canvas.width, height: canvas.height, nonBlank };
  });
  expect(canvasEvidence.painted).toBe(true);
  expect(canvasEvidence.nonBlank).toBeGreaterThan(512);

  const rows = page.locator('tbody .agent-row');
  const agentButtons = page.locator('.agent-select');
  await expect(rows).not.toHaveCount(0);
  await expect(agentButtons.first()).toContainText(/\d+:\d+/);
  await expect(rows.first().locator('td').nth(1)).toHaveText(/-?\d+, -?\d+/);
  expect(await agentButtons.count()).toBeGreaterThan(1);
  await agentButtons.nth(0).focus();
  await agentButtons.nth(0).press('ArrowDown');
  await expect(agentButtons.nth(1)).toBeFocused();
  await expect(agentButtons.nth(1)).toHaveAttribute('aria-pressed', 'true');
  await expect(agentButtons.nth(1).locator('.selection-mark')).toHaveText('▣');
  await expect(agentButtons.nth(1).locator('xpath=ancestor::tr')).toContainText('selected');

  const tick = page.locator('.status-cell').nth(0).locator('strong');
  const hash = page.locator('.hash-value');
  await expect(tick).toHaveText('0');
  const beforeHash = await hash.textContent();
  await page.getByRole('button', { name: 'Step one simulation tick' }).click();
  await expect(tick).toHaveText('1');
  await expect(hash).not.toHaveText(beforeHash ?? '');
  await expect(page.locator('.status-grid .status-value.success')).toContainText('Accepted at tick 1');
  console.log(`browser evidence tick 0 -> ${await tick.textContent()}, hash ${beforeHash} -> ${await hash.textContent()}, reduced-motion active`);

  const motion = await page.locator('*').evaluateAll((elements) => elements.map((element) => {
    const style = getComputedStyle(element);
    return { animationDuration: style.animationDuration, transitionDuration: style.transitionDuration };
  }).filter(({ animationDuration, transitionDuration }) =>
    animationDuration.split(',').some((part) => parseFloat(part) > 0)
    || transitionDuration.split(',').some((part) => parseFloat(part) > 0),
  ));
  expect(motion).toEqual([]);
  await page.screenshot({ path: 'test-results/observatory-live.png', fullPage: true });
});
