import { loadAuth } from './github-auth.js';
import { GitHubStore } from './github.js';
import { registerProvider } from './index.js';

const auth = loadAuth();
if (auth) {
	registerProvider(new GitHubStore(auth.token, auth.login, auth.repo));
}
