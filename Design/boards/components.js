// Draws the parts every Waxbill state shares: the title bar and the sidebar.
// A state is written as <div class="window" data-active="observe" data-serval="ok">
//   <div class="main"> ...screen content and run bar... </div>
// </div>
// and this script inserts the title bar and the sidebar around it.

const NAV = [
  { group: "数据整理" },
  { id: "rename", icon: "✎", label: "重命名" },
  { id: "align", icon: "⇉", label: "路径对齐" },
  { id: "init", icon: "＋", label: "XMP 初始化" },
  { group: "数据分析" },
  { id: "browse", icon: "▦", label: "浏览与精选" },
  { id: "observe", icon: "◉", label: "标签读取" },
  { id: "capture", icon: "◷", label: "独立捕获" },
  { id: "translate", icon: "文", label: "物种名替换" },
  { id: "update", icon: "↻", label: "元数据更新" },
  { id: "sync", icon: "⇄", label: "元数据同步" },
  { group: "工具" },
  { id: "extract", icon: "⇲", label: "媒体提取" },
  { id: "copy", icon: "⧉", label: "XMP 导出" },
  { id: "remove", icon: "⌫", label: "XMP 删除" },
];

const SERVAL = {
  ok: '<span class="pill"><span class="dot ok"></span>Serval 0.8.0</span>',
  update: '<span class="pill warn"><span class="dot warn"></span>Serval 0.8.0 · 有更新</span>',
  missing: '<span class="pill err"><span class="dot err"></span>未找到 Serval</span>',
  old: '<span class="pill err"><span class="dot err"></span>Serval 0.6.7 · 版本过旧</span>',
};

function titlebar(win) {
  const serval = SERVAL[win.dataset.serval || "ok"];
  return `<div class="titlebar">
    <div class="brand"><img src="../../assets/icons/io.github.wsyxbcl.waxbill.png" alt="">Waxbill</div>
    <div class="spacer"></div>
    ${serval}
    <span class="btn ghost" title="设置">⚙</span>
    <span class="win-controls">– □ ✕</span>
  </div>`;
}

function sidebar(win) {
  const active = win.dataset.active;
  const items = NAV.map((n) =>
    n.group
      ? `<div class="group">${n.group}</div>`
      : `<div class="nav${n.id === active ? " active" : ""}"><span class="icon">${n.icon}</span>${n.label}</div>`
  ).join("");
  return `<div class="sidebar">${items}
    <div class="bottom"><div class="nav${active === "settings" ? " active" : ""}"><span class="icon">⚙</span>设置</div></div>
  </div>`;
}

for (const win of document.querySelectorAll(".window")) {
  const main = win.querySelector(":scope > .main");
  const body = document.createElement("div");
  body.className = "body";
  body.innerHTML = sidebar(win);
  body.appendChild(main);
  win.innerHTML = titlebar(win);
  win.appendChild(body);
}
