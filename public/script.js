const canvas = document.getElementById("canvas");
const ctx = canvas.getContext("2d");

canvas.width = window.innerWidth;
canvas.height = window.innerHeight;

let drawing = false;

canvas.addEventListener("mousedown", startDrawing);
canvas.addEventListener("mouseup", stopDrawing);
canvas.addEventListener("mousemove", draw);

function startDrawing(e) {
  drawing = true;
  ctx.beginPath();
  ctx.moveTo(e.clientX, e.clientY);
  console.log("fn: draw", e);
}

function stopDrawing() {
  drawing = false;
  ctx.closePath();
}

function draw(e) {
  if (!drawing) return;

  ctx.lineWidth = 3;
  ctx.lineCap = "round";
  ctx.strokeStyle = "black";
  console.log("fn: draw", e);

  if (e.clientX < 200) {
    ctx.lineWidth = 3;
    ctx.lineCap = "round";
    ctx.strokeStyle = "red";
    ctx.lineTo(e.clientX, e.clientY);
    ctx.stroke();
  } else {
    ctx.lineTo(e.clientX, e.clientY);
    ctx.stroke();
  }
}
