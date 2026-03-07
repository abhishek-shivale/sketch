const canvas = document.getElementById("canvas");
const ctx = canvas.getContext("2d");
const socket = new WebSocket("ws://127.0.0.1:3000/ws");
let user;

canvas.width = window.innerWidth;
canvas.height = window.innerHeight;

let drawing = false;
let otherUserDraw = [];

canvas.addEventListener("mousedown", startDrawing);
canvas.addEventListener("mouseup", stopDrawing);
canvas.addEventListener("mousemove", draw);

async function startDrawing(e) {
  drawing = true;
  ctx.beginPath();
  ctx.moveTo(e.clientX, e.clientY);
  if (socket.readyState === WebSocket.OPEN) {
    socket.send(
      JSON.stringify({
        key: "message",
        value: {
          events: {
            user_started_drawing: {
              x: e.clientX,
              y: e.clientY,
            },
          },
        },
        user,
      }),
    );
  }
}

function stopDrawing() {
  drawing = false;
  ctx.closePath();
  if (socket.readyState === WebSocket.OPEN) {
    socket.send(
      JSON.stringify({
        key: "message",
        value: {
          events: {
            user_stopped_drawing: {},
          },
        },
        user,
      }),
    );
  }
}

function draw(e) {
  if (!drawing) return;

  ctx.lineWidth = 3;
  ctx.lineCap = "round";
  ctx.strokeStyle = user.color ?? "black";

  ctx.lineTo(e.clientX, e.clientY);
  ctx.stroke();

  if (socket.readyState === WebSocket.OPEN) {
    socket.send(
      JSON.stringify({
        key: "message",
        value: {
          events: {
            user_is_drawing: {
              x: e.clientX,
              y: e.clientY,
            },
          },
        },
        user,
      }),
    );
  }
}
// if(msf)

socket.addEventListener("message", (msg) => {
  const data = JSON.parse(msg.data);

  switch (data.key) {
    case "connected": {
      user = data.user;
      break;
    }
    case "message": {
      if (data.user.id === user.id) return;
      const eventKey = Object.keys(data.value.events)[0];
      switch (eventKey) {
        case "user_started_drawing": {
          const draw = data.value.events.user_started_drawing;
          ctx.beginPath();
          console.log(draw)
          ctx.moveTo(draw.x, draw.y);
          break;
        }
        case "user_is_drawing": {
          const draw = data.value.events.user_is_drawing;
          ctx.lineWidth = 3;
          ctx.lineCap = "round";
          ctx.strokeStyle = data.user.color ?? "green";
          console.log(draw)
          ctx.lineTo(draw.x, draw.y);
          ctx.stroke();
          break;
        }
        case "user_stopped_drawing": {
          ctx.closePath();
          break;
        }
      }
    }
    case "disconnect": {
    }
    default: {
    }
  }
});
