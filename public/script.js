const canvas = document.getElementById("canvas");
const ctx = canvas.getContext("2d");
const socket = new WebSocket("ws://127.0.0.1:3000/ws");
const USER_ID = Math.floor(Math.random() * 10);

canvas.width = window.innerWidth;
canvas.height = window.innerHeight;

let drawing = false;

canvas.addEventListener("mousedown", startDrawing);
canvas.addEventListener("mouseup", stopDrawing);
canvas.addEventListener("mousemove", draw);

async function startDrawing(e) {
  drawing = true;
  ctx.beginPath();
  ctx.moveTo(e.clientX, e.clientY);
}

function stopDrawing() {
  drawing = false;
  ctx.closePath();
}

function draw(e) {
  if (!drawing) return;

  ctx.lineWidth = 3;
  ctx.lineCap = "round";
  ctx.strokeStyle = e.clientX < 200 ? "red" : "black";

  ctx.lineTo(e.clientX, e.clientY);
  ctx.stroke();

  if (socket.readyState === WebSocket.OPEN) {
    socket.send(
      JSON.stringify({
        key: USER_ID,
        value: {
          x: e.clientX,
          y: e.clientY,
        },
      }),
    );
  }
}
// if(msf)

socket.addEventListener("message", (msg) => {
  const data = JSON.parse(msg.data);
  
  console.log(data.key, USER_ID)

  if (data.key !== USER_ID) {
    ctx.lineWidth = 3;
    ctx.lineCap = "round";
    ctx.strokeStyle = "blue";

    ctx.lineTo(data.value.x.clientX, data.value.y.clientY);
    ctx.stroke();
  }
});
