import '@fontsource-variable/inter';
import '@fontsource/pixelify-sans/600.css';
import '@fontsource/jetbrains-mono/400.css';
import './app.css';
import { mount } from 'svelte';
import App from './App.svelte';

mount(App, { target: /** @type {HTMLElement} */ (document.getElementById('app')) });
