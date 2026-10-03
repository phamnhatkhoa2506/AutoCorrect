<script>
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';

  const tabs = [
    ['general', 'Chung'],
    ['strength', 'Mức độ sửa'],
    ['apps', 'Ứng dụng'],
    ['dictionary', 'Từ điển'],
    ['about', 'Thông tin'],
  ];
  const strengths = [
    ['careful', 'Cẩn thận', 'Chỉ sửa khi rất chắc. Ít sửa nhầm nhất, bỏ sót nhiều hơn.'],
    ['balanced', 'Cân bằng', 'Mặc định: sửa phần lớn lỗi gõ, hiếm khi sửa nhầm.'],
    ['bold', 'Mạnh tay', 'Sửa nhiều hơn, kể cả lỗi hai phím; đổi lại thi thoảng sửa nhầm.'],
  ];
  const kinds = [
    ['normal', 'Bình thường'],
    ['code', 'Terminal / IDE (chỉ sửa tiếng Việt)'],
    ['off', 'Tắt hẳn'],
  ];

  let tab = $state('general');
  let loaded = $state(false);
  let options = $state(null);
  let apps = $state({ kinds: [], guards: [] });
  let dictionary = $state({ ignore: [], fixes: [] });
  let autostart = $state(false);
  let configDir = $state('');
  let message = $state('');

  let capturing = $state(false);
  let preview = $state('');
  let held = new Set();
  let usedKey = false;
  let peak = 0;
  let lastPreview = '';

  let newApp = $state('');
  let newKind = $state('code');
  let newGuardApp = $state('');
  let newGuard = $state(true);
  let newIgnore = $state('');
  let newTyped = $state('');
  let newInstead = $state('');

  onMount(async () => {
    const s = await invoke('get_state');
    options = s.options;
    apps = s.apps;
    dictionary = s.dictionary;
    autostart = s.autostart;
    configDir = s.config_dir;
    loaded = true;
  });

  function flash(text) {
    message = text;
    setTimeout(() => {
      if (message === text) message = '';
    }, 2500);
  }

  async function saveOptions() {
    try {
      await invoke('save_options', { options: $state.snapshot(options) });
      flash('Đã lưu');
    } catch (e) {
      flash(String(e));
    }
  }
  const saveApps = () => invoke('save_apps', { apps: $state.snapshot(apps) }).then(() => flash('Đã lưu'));
  const saveDictionary = () =>
    invoke('save_dictionary', { dictionary: $state.snapshot(dictionary) }).then(() => flash('Đã lưu'));
  const saveAutostart = () => invoke('set_autostart', { on: autostart });

  // ---- hotkey capture -------------------------------------------------
  const MODS = { Control: 'Ctrl', Shift: 'Shift', Alt: 'Alt', Meta: 'Win' };
  const ORDER = ['Ctrl', 'Shift', 'Alt', 'Win'];

  function keyName(e) {
    if (/^Key[A-Z]$/.test(e.code)) return e.code.slice(3);
    if (/^Digit[0-9]$/.test(e.code)) return e.code.slice(5);
    if (/^F([1-9]|1[0-2])$/.test(e.code)) return e.code;
    if (['Space', 'Tab', 'Backquote'].includes(e.code)) return e.code;
    return null;
  }

  function show() {
    preview = ORDER.filter((m) => held.has(m)).join('+');
    if (preview) lastPreview = preview;
  }

  function startCapture() {
    held = new Set();
    usedKey = false;
    peak = 0;
    preview = '';
    lastPreview = '';
    capturing = true;
  }

  async function commit(text) {
    capturing = false;
    try {
      options.hotkey = await invoke('check_hotkey', { text });
      await saveOptions();
    } catch (e) {
      flash(String(e));
    }
  }

  function onKeyDown(e) {
    if (!capturing) return;
    e.preventDefault();
    if (e.key === 'Escape') {
      capturing = false;
      return;
    }
    if (MODS[e.key]) {
      held.add(MODS[e.key]);
      peak = Math.max(peak, held.size);
      show();
      return;
    }
    const name = keyName(e);
    if (!name) {
      flash('Phím này chưa hỗ trợ: dùng chữ, số, Space, Tab, ` hoặc F1–F12');
      return;
    }
    usedKey = true;
    commit([...ORDER.filter((m) => held.has(m)), name].join('+'));
  }

  function onKeyUp(e) {
    if (!capturing || !MODS[e.key]) return;
    e.preventDefault();
    const was = lastPreview;
    held.delete(MODS[e.key]);
    if (held.size === 0 && !usedKey && peak >= 2) {
      // Only modifiers, all released: that is the combination (Ctrl+Shift).
      commit(was);
    } else {
      show();
    }
  }

  // ---- apps & dictionary ----------------------------------------------
  function addApp() {
    const name = newApp.trim().toLowerCase();
    if (!name) return;
    apps.kinds = [...apps.kinds.filter((r) => r.name !== name), { name, kind: newKind }];
    newApp = '';
    saveApps();
  }
  function removeApp(i) {
    apps.kinds = apps.kinds.filter((_, j) => j !== i);
    saveApps();
  }
  function addGuard() {
    const name = newGuardApp.trim().toLowerCase();
    if (!name) return;
    apps.guards = [...apps.guards.filter((r) => r.name !== name), { name, guard: newGuard }];
    newGuardApp = '';
    saveApps();
  }
  function removeGuard(i) {
    apps.guards = apps.guards.filter((_, j) => j !== i);
    saveApps();
  }
  function addIgnore() {
    const w = newIgnore.trim().toLowerCase();
    if (!w || dictionary.ignore.includes(w)) return;
    dictionary.ignore = [...dictionary.ignore, w];
    newIgnore = '';
    saveDictionary();
  }
  function removeIgnore(i) {
    dictionary.ignore = dictionary.ignore.filter((_, j) => j !== i);
    saveDictionary();
  }
  function addFix() {
    const typed = newTyped.trim().toLowerCase();
    const instead = newInstead.trim();
    if (!typed || !instead) return;
    dictionary.fixes = [...dictionary.fixes.filter((f) => f.typed !== typed), { typed, instead }];
    newTyped = '';
    newInstead = '';
    saveDictionary();
  }
  function removeFix(i) {
    dictionary.fixes = dictionary.fixes.filter((_, j) => j !== i);
    saveDictionary();
  }
</script>

<svelte:window onkeydown={onKeyDown} onkeyup={onKeyUp} />

{#if loaded}
  <div class="shell">
    <nav>
      {#each tabs as [id, label]}
        <button class:active={tab === id} onclick={() => (tab = id)}>{label}</button>
      {/each}
    </nav>

    <main>
      {#if tab === 'general'}
        <h2>Chung</h2>
        <section>
          <h3>Phím tắt chuyển Việt / Anh</h3>
          <div class="hotkey">
            <button class="keycap" class:capturing onclick={startCapture}>
              {#if capturing}{preview || 'Nhấn tổ hợp phím...'}{:else}{options.hotkey}{/if}
            </button>
            <span class="hint">
              {#if capturing}Esc để huỷ{:else}Bấm rồi nhấn tổ hợp mới: Ctrl+Shift, Alt+Z, Ctrl+Space...{/if}
            </span>
          </div>
        </section>
        <section>
          <h3>Kiểu gõ</h3>
          <label><input type="radio" name="input" value="telex" bind:group={options.input} onchange={saveOptions} />
            <span>Telex <small>aa â, ow ơ, dd đ; s f r x j là dấu</small></span></label>
          <label><input type="radio" name="input" value="vni" bind:group={options.input} onchange={saveOptions} />
            <span>VNI <small>a6 â, o7 ơ, d9 đ; 1 2 3 4 5 là dấu</small></span></label>
        </section>
        <section>
          <h3>Tuỳ chọn</h3>
          <label><input type="checkbox" bind:checked={options.corrections} onchange={saveOptions} />
            <span>Tự sửa lỗi gõ khi nhấn Space
              <small>Tắt đi thì chỉ còn gõ Telex.</small></span></label>
          <label><input type="checkbox" bind:checked={options.restore_marks} onchange={saveOptions} />
            <span>Tự thêm dấu khi gõ không dấu <small>khong → không</small></span></label>
          <label><input type="checkbox" bind:checked={options.code_english} onchange={saveOptions} />
            <span>Sửa lỗi tiếng Anh cả trong IDE / terminal
              <small>Hữu ích cho khung chat của IDE; có thể đụng tới lệnh và mã.</small></span></label>
          <label><input type="checkbox" bind:checked={options.autocomplete_guard} onchange={saveOptions} />
            <span>Chống lỗi gợi ý trong trình duyệt / ô tìm kiếm
              <small>Tránh chữ bị lặp khi trình duyệt tự điền.</small></span></label>
          <label><input type="checkbox" bind:checked={options.journal} onchange={saveOptions} />
            <span>Ghi nhật ký sửa lỗi <small>Để tinh chỉnh sau này; chỉ lưu trên máy này.</small></span></label>
          <label><input type="checkbox" bind:checked={options.journal_edits} onchange={saveOptions} />
            <span>Ghi chữ bạn tự sửa tay
              <small>Khi bạn xóa lùi vào một từ rồi sửa. Lưu từ trước và sau khi sửa. Không ghi trong terminal/IDE. Mặc định tắt.</small></span></label>
          <label><input type="checkbox" bind:checked={options.journal_hard} onchange={saveOptions} />
            <span>Ghi ca khó app bỏ qua
              <small>Từ gõ, 3 từ trước đó và điểm các ứng viên gần nhau. Không ghi trong terminal/IDE. Tối đa 8 MB. Mặc định tắt.</small></span></label>
          <label><input type="checkbox" bind:checked={autostart} onchange={saveAutostart} />
            <span>Khởi động cùng Windows</span></label>
        </section>
      {:else if tab === 'strength'}
        <h2>Mức độ sửa</h2>
        <p class="lead">Chọn mức bạn thấy thoải mái. Sửa nhầm gây phiền hơn bỏ sót nên mặc định là cân bằng.</p>
        {#each strengths as [id, name, text]}
          <label class="card" class:chosen={options.strength === id}>
            <input type="radio" name="strength" value={id} bind:group={options.strength} onchange={saveOptions} />
            <span><b>{name}</b><small>{text}</small></span>
          </label>
        {/each}
      {:else if tab === 'apps'}
        <h2>Ứng dụng</h2>
        <p class="lead">
          Chương trình có sẵn đã được xếp nhóm (terminal, IDE, ứng dụng chống sửa...). Thêm ở đây để ghi đè.
          Tên là tên file chạy, ví dụ <code>code.exe</code>.
        </p>
        <section>
          <h3>Nhóm</h3>
          {#each apps.kinds as rule, i}
            <div class="row">
              <code>{rule.name}</code>
              <select bind:value={rule.kind} onchange={saveApps}>
                {#each kinds as [id, label]}<option value={id}>{label}</option>{/each}
              </select>
              <button class="x" onclick={() => removeApp(i)} aria-label="Xoá">×</button>
            </div>
          {:else}
            <p class="empty">Chưa có ghi đè nào.</p>
          {/each}
          <form class="add" onsubmit={(e) => { e.preventDefault(); addApp(); }}>
            <input placeholder="tên.exe" bind:value={newApp} />
            <select bind:value={newKind}>{#each kinds as [id, label]}<option value={id}>{label}</option>{/each}</select>
            <button>Thêm</button>
          </form>
        </section>
        <section>
          <h3>Chống lỗi gợi ý</h3>
          {#each apps.guards as rule, i}
            <div class="row">
              <code>{rule.name}</code>
              <select bind:value={rule.guard} onchange={saveApps}>
                <option value={true}>Bật</option>
                <option value={false}>Tắt</option>
              </select>
              <button class="x" onclick={() => removeGuard(i)} aria-label="Xoá">×</button>
            </div>
          {:else}
            <p class="empty">Chưa có ghi đè nào.</p>
          {/each}
          <form class="add" onsubmit={(e) => { e.preventDefault(); addGuard(); }}>
            <input placeholder="tên.exe" bind:value={newGuardApp} />
            <select bind:value={newGuard}><option value={true}>Bật</option><option value={false}>Tắt</option></select>
            <button>Thêm</button>
          </form>
        </section>
      {:else if tab === 'dictionary'}
        <h2>Từ điển cá nhân</h2>
        <p class="lead">Viết đúng như bạn gõ phím (tiếng Việt thì là phím Telex), chữ thường.</p>
        <section>
          <h3>Không bao giờ sửa</h3>
          <div class="chips">
            {#each dictionary.ignore as word, i}
              <span class="chip">{word}<button onclick={() => removeIgnore(i)} aria-label="Xoá">×</button></span>
            {:else}
              <p class="empty">Chưa có từ nào. App tự thêm khi bạn hoàn tác cùng một lần sửa hai lần.</p>
            {/each}
          </div>
          <form class="add" onsubmit={(e) => { e.preventDefault(); addIgnore(); }}>
            <input placeholder="vd: kubectl" bind:value={newIgnore} />
            <button>Thêm</button>
          </form>
        </section>
        <section>
          <h3>Luôn đổi thành</h3>
          {#each dictionary.fixes as fix, i}
            <div class="row">
              <code>{fix.typed}</code><span class="arrow">→</span><b>{fix.instead}</b>
              <button class="x" onclick={() => removeFix(i)} aria-label="Xoá">×</button>
            </div>
          {:else}
            <p class="empty">Chưa có mục nào.</p>
          {/each}
          <form class="add" onsubmit={(e) => { e.preventDefault(); addFix(); }}>
            <input placeholder="gõ (vd: ko)" bind:value={newTyped} />
            <span class="arrow">→</span>
            <input placeholder="thành (vd: không)" bind:value={newInstead} />
            <button>Thêm</button>
          </form>
        </section>
      {:else}
        <h2>Thông tin</h2>
        <section>
          <p><b>AutoCorrect</b> — gõ Telex và sửa lỗi gõ tiếng Việt / tiếng Anh theo thời gian thực, chạy hoàn toàn trên máy.</p>
          <p>Cấu hình lưu tại:<br /><code>{configDir}</code></p>
          <button onclick={() => invoke('open_config_dir')}>Mở thư mục cấu hình</button>
          <p class="hint">Thay đổi có hiệu lực khi bạn chuyển sang cửa sổ khác; không cần khởi động lại app.</p>
        </section>
      {/if}
    </main>

    {#if message}<div class="toast">{message}</div>{/if}
  </div>
{/if}

<style>
  :global(:root) {
    --bg: #f5f6f8;
    --panel: #ffffff;
    --text: #1d2330;
    --muted: #667085;
    --line: #e3e6ec;
    --accent: #2563eb;
    --accent-soft: #e8efff;
    font-family: 'Segoe UI', system-ui, sans-serif;
    font-size: 14px;
    color: var(--text);
    background: var(--bg);
  }
  @media (prefers-color-scheme: dark) {
    :global(:root) {
      --bg: #14171f;
      --panel: #1c2029;
      --text: #e6e9f0;
      --muted: #98a2b3;
      --line: #2b313d;
      --accent: #6b9bff;
      --accent-soft: #232c45;
    }
  }
  :global(body) {
    margin: 0;
    background: var(--bg);
    color: var(--text);
  }
  .shell {
    display: grid;
    grid-template-columns: 170px 1fr;
    height: 100vh;
  }
  nav {
    background: var(--panel);
    border-right: 1px solid var(--line);
    padding: 16px 10px;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  nav button {
    text-align: left;
    background: none;
    border: 0;
    padding: 10px 12px;
    border-radius: 8px;
    color: var(--text);
    font: inherit;
    cursor: pointer;
  }
  nav button:hover { background: var(--accent-soft); }
  nav button.active { background: var(--accent-soft); color: var(--accent); font-weight: 600; }
  main { padding: 20px 28px; overflow-y: auto; }
  h2 { margin: 0 0 12px; font-size: 20px; }
  h3 { margin: 0 0 10px; font-size: 13px; text-transform: uppercase; letter-spacing: 0.04em; color: var(--muted); }
  section {
    background: var(--panel);
    border: 1px solid var(--line);
    border-radius: 12px;
    padding: 16px 18px;
    margin-bottom: 16px;
  }
  .lead { color: var(--muted); margin: 0 0 14px; line-height: 1.5; }
  label { display: flex; gap: 10px; align-items: flex-start; padding: 7px 0; cursor: pointer; }
  label span { display: flex; flex-direction: column; gap: 2px; }
  small { color: var(--muted); font-size: 12px; }
  input[type='checkbox'], input[type='radio'] { margin-top: 3px; accent-color: var(--accent); }
  .card {
    background: var(--panel);
    border: 1px solid var(--line);
    border-radius: 12px;
    padding: 14px 16px;
    margin-bottom: 10px;
  }
  .card.chosen { border-color: var(--accent); background: var(--accent-soft); }
  .hotkey { display: flex; align-items: center; gap: 14px; flex-wrap: wrap; }
  .keycap {
    min-width: 150px;
    padding: 10px 18px;
    font: 600 15px 'Segoe UI', sans-serif;
    border-radius: 10px;
    border: 1px solid var(--line);
    border-bottom-width: 3px;
    background: var(--bg);
    color: var(--text);
    cursor: pointer;
  }
  .keycap.capturing { border-color: var(--accent); color: var(--accent); }
  .hint, .empty { color: var(--muted); font-size: 12px; }
  .row { display: flex; align-items: center; gap: 10px; padding: 6px 0; border-bottom: 1px solid var(--line); }
  .row code { flex: 1; }
  .arrow { color: var(--muted); }
  .add { display: flex; gap: 8px; margin-top: 12px; align-items: center; }
  .add input { flex: 1; }
  input:not([type]), select {
    font: inherit;
    padding: 7px 10px;
    border-radius: 8px;
    border: 1px solid var(--line);
    background: var(--bg);
    color: var(--text);
    min-width: 0;
  }
  section button:not(.x):not(.keycap):not(.chip button) {
    font: inherit;
    padding: 7px 14px;
    border-radius: 8px;
    border: 0;
    background: var(--accent);
    color: white;
    cursor: pointer;
  }
  .x { background: none; border: 0; color: var(--muted); font-size: 18px; cursor: pointer; }
  .x:hover { color: #d92d20; }
  .chips { display: flex; flex-wrap: wrap; gap: 6px; }
  .chip { display: inline-flex; align-items: center; gap: 4px; background: var(--accent-soft); border-radius: 999px; padding: 3px 4px 3px 12px; }
  .chip button { background: none; border: 0; color: var(--muted); cursor: pointer; font-size: 15px; }
  code { font-family: 'Cascadia Mono', Consolas, monospace; font-size: 13px; }
  .toast {
    position: fixed; bottom: 18px; right: 22px;
    background: var(--text); color: var(--bg);
    padding: 8px 14px; border-radius: 8px; font-size: 13px;
  }
</style>
