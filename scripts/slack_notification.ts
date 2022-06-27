import 'dotenv/config'
// @ts-ignore
import SlackNotify from 'slack-notify';

const MY_SLACK_WEBHOOK_URL = process.env.SLACK_WEBHOOK_URL
const slack = SlackNotify(MY_SLACK_WEBHOOK_URL);

export async function sendNotification(name: string, msg: string, stack: string | undefined) {
    if (process.env.SLACK_WEBHOOK_URL!) {
        slack.alert({
            text: name,
            fields: {
                'message': msg,
                'stack': stack
            }
        });
    } else {
        console.error(`${name} - ${msg} \n${stack}`)
    }
}

sendNotification("hello", "message", "some info").catch(console.log)