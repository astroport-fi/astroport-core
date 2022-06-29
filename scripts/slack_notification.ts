import 'dotenv/config'
// @ts-ignore
import SlackNotify from 'slack-notify';

const MY_SLACK_WEBHOOK_URL = process.env.SLACK_WEBHOOK_URL
const slack = SlackNotify(MY_SLACK_WEBHOOK_URL);

async function sendNotification(msg: string) {
    if (process.env.SLACK_WEBHOOK_URL!) {
        slack.send(msg)
            .then(() => {
                console.log('done!');
            })
            .catch((err: any) => {
                console.error(err);
            });
    } else {
        console.error(`Slack webhook url not found: ${MY_SLACK_WEBHOOK_URL}`)
    }
}

sendNotification(process.argv[2]).catch(console.log)