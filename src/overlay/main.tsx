import { createRoot } from 'react-dom/client';
import { OverlayApp } from './OverlayApp';

const rootElement = document.getElementById('overlay-root');

if (!rootElement) {
  throw new Error('Missing #overlay-root mount point');
}

createRoot(rootElement).render(<OverlayApp />);
