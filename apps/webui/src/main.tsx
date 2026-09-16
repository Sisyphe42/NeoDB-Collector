import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import { App } from './App.js';
import { createAccessClient, createCoreClient } from '@neodb-collector/core-client';
import { AccessPanel } from './AccessPanel.js';
import { startSession } from './session.js';

const root = document.getElementById('root');
if (!root) throw new Error('Missing WebUI root element.');
const access = createAccessClient();
const initialSession = startSession(access, window.location, window.history, createCoreClient().status);
// Preserve the rejection for the panel, even if incompatible Core prevents mounting it.
void initialSession.catch(() => undefined);
createRoot(root).render(<StrictMode><App access={<AccessPanel client={access} initialSession={initialSession} />} /></StrictMode>);
