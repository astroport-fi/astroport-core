module.exports = ({ file, options, env }) => ({
    parser: false,
    plugins: {
        'postcss-import': {},
        'postcss-cssnext': {},
        'cssnano':  env === 'production'  ? {} : false
    }
});