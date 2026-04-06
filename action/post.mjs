import * as fs from "node:fs";
import * as path from "node:path";
import {
  getState,
  LOGS_DIR,
  SERVER_PID_KEY,
  isProcessRunning,
  sleep,
} from "./util.mjs";

let pid = getState(SERVER_PID_KEY);

if (typeof pid === "undefined") {
  console.error(`${SERVER_PID_KEY} state could not be found. Exiting...`);
  process.exit(1);
}

pid = parseInt(pid);

console.log(`Rush Cache Server will be stopped on pid: ${pid}`);

if (!isProcessRunning(pid)) {
  console.log(`Process ${pid} is not running. It may have already exited.`);
  // Continue to read logs anyway
} else {
  try {
    process.kill(pid, "SIGTERM");

    const maxProcessCheckAttempts = 20;
    const sleepTimeInMills = 500;
    let killCounter = 0;
    while (isProcessRunning(pid)) {
      if (killCounter >= maxProcessCheckAttempts) {
        console.error("Taking too long to stop. Killing it directly");
        process.kill(pid, "SIGKILL");
        break;
      }
      console.log(`Server is shutting down. Waiting ${sleepTimeInMills}ms...`);
      await sleep(sleepTimeInMills);
      killCounter = killCounter + 1;
    }
  } catch (err) {
    switch (err.code) {
      case "ESRCH": {
        console.log(`Process ${pid} no longer exists. Skipping kill.`);
        break;
      }
      case "EPERM": {
        console.error(`Permission denied to kill process ${pid}.`);
        break;
      }
      default: {
        console.error("Error not mapped", err);
        throw err;
      }
    }
  }
}

// Read logs and output so we can debug any potential errors
const logFile = path.resolve(LOGS_DIR, "rush-cache-server.log");
console.log(`Reading Rush Cache Server logs from ${logFile}`);
try {
  const serverLogs = fs.readFileSync(logFile, { encoding: "utf-8" });
  console.log(serverLogs);
} catch (err) {
  console.log(`Could not read log file: ${err.message}`);
}

process.exit(0);
