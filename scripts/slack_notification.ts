// @ts-ignore
import SlackNotify from 'slack-notify';

const MY_SLACK_WEBHOOK_URL = process.env.SLACK_WEBHOOK_URL!
const slack = SlackNotify(MY_SLACK_WEBHOOK_URL);

export async function sendNotification(msg: string) {
    slack.send(msg)
        .then(() => {
            console.log('done!');
        }).catch((err: any) => {
        console.error(err);
    });
}
