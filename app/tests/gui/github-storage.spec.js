import { test as rawTest, expect } from '@playwright/test';

/**
 * Mock GitHub API endpoints for testing.
 * Call before navigating to the app.
 */
async function mockGitHubAPI(page, opts = {}) {
	const { hasIndex = false, indexDocs = {} } = opts;

	// Mock device code request
	await page.route('https://github.com/login/device/code', async (route) => {
		await route.fulfill({
			status: 200,
			contentType: 'application/json',
			body: JSON.stringify({
				device_code: 'test-device-code-123',
				user_code: 'ABCD-1234',
				verification_uri: 'https://github.com/login/device',
				expires_in: 900,
				interval: 1
			})
		});
	});

	// Mock token poll — pending once, then success
	let pollCount = 0;
	await page.route('https://github.com/login/oauth/access_token', async (route) => {
		pollCount++;
		if (pollCount <= 1) {
			await route.fulfill({
				status: 200,
				contentType: 'application/json',
				body: JSON.stringify({ error: 'authorization_pending' })
			});
		} else {
			await route.fulfill({
				status: 200,
				contentType: 'application/json',
				body: JSON.stringify({ access_token: 'ghu_test_token_123', token_type: 'bearer' })
			});
		}
	});

	// Mock user endpoint
	await page.route('https://api.github.com/user', async (route) => {
		await route.fulfill({
			status: 200,
			contentType: 'application/json',
			body: JSON.stringify({ login: 'testuser', avatar_url: 'https://example.com/avatar.png' })
		});
	});

	// Mock repo check
	await page.route('https://api.github.com/repos/testuser/waffle-iron-documents', async (route) => {
		if (route.request().method() === 'GET') {
			await route.fulfill({
				status: 200,
				contentType: 'application/json',
				body: JSON.stringify({ name: 'waffle-iron-documents', full_name: 'testuser/waffle-iron-documents', private: false })
			});
		}
	});

	// Mock index file
	await page.route('https://api.github.com/repos/testuser/waffle-iron-documents/contents/.waffle-index.json', async (route) => {
		if (route.request().method() === 'GET') {
			if (hasIndex) {
				const content = btoa(JSON.stringify({ version: 1, documents: indexDocs }));
				await route.fulfill({
					status: 200,
					contentType: 'application/json',
					body: JSON.stringify({ content, sha: 'abc123', encoding: 'base64' })
				});
			} else {
				await route.fulfill({ status: 404, contentType: 'application/json', body: '{"message":"Not Found"}' });
			}
		} else {
			await route.fulfill({
				status: 200,
				contentType: 'application/json',
				body: JSON.stringify({ content: { sha: 'def456' } })
			});
		}
	});

	// Mock file operations (catch-all for contents)
	await page.route('https://api.github.com/repos/testuser/waffle-iron-documents/contents/*.waffle', async (route) => {
		if (route.request().method() === 'PUT') {
			await route.fulfill({
				status: 200,
				contentType: 'application/json',
				body: JSON.stringify({ content: { sha: 'newsha123' } })
			});
		} else if (route.request().method() === 'GET') {
			const content = btoa(JSON.stringify({ format: 'waffle-iron', version: 3, document: { name: 'Test' }, tabs: [], active_tab: null }));
			await route.fulfill({
				status: 200,
				contentType: 'application/json',
				body: JSON.stringify({ content, sha: 'filesha', encoding: 'base64' })
			});
		} else if (route.request().method() === 'DELETE') {
			await route.fulfill({ status: 200, contentType: 'application/json', body: '{}' });
		}
	});
}

rawTest.describe('GitHub storage provider', () => {
	rawTest('provider dropdown shows on home page', async ({ page }) => {
		await page.goto('/home');
		await expect(page.locator('[data-testid="provider-dropdown"]')).toBeVisible({ timeout: 10000 });
		await expect(page.locator('[data-testid="home-header"]')).toBeVisible();
	});

	rawTest('provider dropdown shows local provider by default', async ({ page }) => {
		await page.goto('/home');
		const trigger = page.locator('[data-testid="provider-dropdown"] button').first();
		await expect(trigger).toContainText('This Browser');
	});

	rawTest('clicking dropdown shows providers and Connect GitHub option', async ({ page }) => {
		await page.goto('/home');
		await page.waitForTimeout(500);
		const trigger = page.locator('[data-testid="provider-dropdown"] button').first();
		await trigger.click();
		await expect(page.locator('[data-testid="provider-option-local"]')).toBeVisible();
		await expect(page.locator('[data-testid="provider-connect-github"]')).toBeVisible();
	});

	rawTest('Connect GitHub opens dialog', async ({ page }) => {
		await mockGitHubAPI(page);
		await page.goto('/home');
		await page.waitForTimeout(500);
		const trigger = page.locator('[data-testid="provider-dropdown"] button').first();
		await trigger.click();
		await page.locator('[data-testid="provider-connect-github"]').click();
		await expect(page.locator('[data-testid="github-connect-dialog"]')).toBeVisible();
	});

	rawTest('device flow shows user code after clicking connect', async ({ page }) => {
		await mockGitHubAPI(page);
		await page.goto('/home');
		await page.waitForTimeout(500);
		// Open dialog
		await page.locator('[data-testid="provider-dropdown"] button').first().click();
		await page.locator('[data-testid="provider-connect-github"]').click();
		await expect(page.locator('[data-testid="github-connect-dialog"]')).toBeVisible();

		// Click connect
		await page.locator('[data-testid="github-connect-btn"]').click();
		// Should show user code
		await expect(page.locator('[data-testid="github-user-code"]')).toBeVisible({ timeout: 5000 });
		await expect(page.locator('[data-testid="github-user-code"]')).toContainText('ABCD-1234');
	});
});
