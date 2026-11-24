const agree = document.getElementById('agree');
const accept = document.getElementById('accept');
const error = document.getElementById('error');

agree.addEventListener('change', () => { accept.disabled = !agree.checked; });

accept.addEventListener('click', async () => {
  accept.disabled = true;
  try {
    await window.__TAURI__.core.invoke('accept_terms');
    window.location.replace('index.html');
  } catch (e) {
    error.textContent = `Could not save your acceptance: ${e}`;
    accept.disabled = !agree.checked;
  }
});
